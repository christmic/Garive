use std::{
    ffi::c_void,
    fs::{self, File},
    io,
    mem::{size_of, zeroed},
    os::windows::{ffi::OsStrExt, fs::MetadataExt, io::FromRawHandle},
    path::Path,
    ptr::{null, null_mut},
    slice,
};

use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND,
        ERROR_PATH_NOT_FOUND, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    },
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        EqualSid, GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetTokenInformation,
        IsValidAcl, TokenUser, ACL, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, SE_DACL_PROTECTED, TOKEN_QUERY,
        TOKEN_USER,
    },
    Storage::FileSystem::{
        CreateDirectoryW, CreateFileW, GetFileInformationByHandle, MoveFileExW,
        BY_HANDLE_FILE_INFORMATION, CREATE_NEW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, MOVEFILE_REPLACE_EXISTING,
        MOVEFILE_WRITE_THROUGH, OPEN_ALWAYS, OPEN_EXISTING, READ_CONTROL,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

use super::StateError;
const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
pub(crate) fn create_private_directory(path: &Path) -> Result<(), StateError> {
    let path = std::path::absolute(path).map_err(|_| StateError::Unavailable)?;
    let mut missing = Vec::new();
    let mut cursor = path.clone();
    loop {
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) => {
                if !metadata.is_dir()
                    || metadata.file_type().is_symlink()
                    || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                {
                    return Err(StateError::UnsafePermissions);
                }
                reject_reparse_ancestors(&cursor)?;
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing.push(cursor.clone());
                cursor = cursor
                    .parent()
                    .map(Path::to_path_buf)
                    .ok_or(StateError::Unavailable)?;
            }
            Err(_) => return Err(StateError::Unavailable),
        }
    }
    if missing.is_empty() {
        return validate_path(&path, true);
    }
    for directory in missing.iter().rev() {
        create_one_directory(directory)?;
        validate_path(directory, true)?;
    }
    Ok(())
}
fn reject_reparse_ancestors(path: &Path) -> Result<(), StateError> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(|_| StateError::Unavailable)?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(StateError::UnsafePermissions);
        }
    }
    Ok(())
}
pub(crate) fn validate_private_file_if_present(path: &Path) -> Result<(), StateError> {
    match fs::symlink_metadata(path) {
        Ok(_) => validate_path(path, false),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(StateError::Unavailable),
    }
}
pub(crate) fn private_create_new(path: &Path) -> Result<File, StateError> {
    secure_open(path, GENERIC_WRITE, CREATE_NEW)
}

pub(crate) fn private_open(path: &Path) -> Result<File, StateError> {
    secure_open(path, GENERIC_READ | GENERIC_WRITE, OPEN_ALWAYS)
}

pub(crate) fn private_append(path: &Path) -> Result<File, StateError> {
    secure_open(path, FILE_APPEND_DATA, OPEN_ALWAYS)
}
pub(super) fn durable_rename(
    source: &Path,
    target: &Path,
    replace: bool,
) -> Result<(), StateError> {
    let source = wide_path(source)?;
    let target = wide_path(target)?;
    let mut flags = MOVEFILE_WRITE_THROUGH;
    if replace {
        flags |= MOVEFILE_REPLACE_EXISTING;
    }
    // SAFETY: both buffers are owned, absolute, NUL-terminated paths and stay
    // alive for the duration of the synchronous call.
    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), flags) } == 0 {
        return Err(StateError::Unavailable);
    }
    Ok(())
}
fn create_one_directory(path: &Path) -> Result<(), StateError> {
    let descriptor = SecurityDescriptor::private(true)?;
    let attributes = descriptor.attributes();
    let path = wide_path(path)?;
    // SAFETY: path and descriptor allocations outlive this synchronous call;
    // SECURITY_ATTRIBUTES points at the descriptor owned above.
    if unsafe { CreateDirectoryW(path.as_ptr(), &attributes) } != 0 {
        return Ok(());
    }
    // SAFETY: read immediately after the failed Win32 call on this thread.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return Ok(());
    }
    Err(StateError::Unavailable)
}
fn secure_open(path: &Path, access: u32, disposition: u32) -> Result<File, StateError> {
    let descriptor = SecurityDescriptor::private(false)?;
    let attributes = descriptor.attributes();
    let path = wide_path(path)?;
    // SAFETY: all pointer arguments are either null or point to live,
    // NUL-terminated/initialized values for the complete synchronous call.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            SHARE_ALL,
            &attributes,
            disposition,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(StateError::Unavailable);
    }
    let handle = OwnedHandle(handle);
    validate_handle(handle.0, false)?;
    let raw = handle.into_raw();
    // SAFETY: CreateFileW returned a valid uniquely owned handle. into_raw
    // suppresses the guard's close, transferring exactly one close to File.
    Ok(unsafe { File::from_raw_handle(raw.cast()) })
}
fn validate_path(path: &Path, expected_directory: bool) -> Result<(), StateError> {
    let path = wide_path(path)?;
    // SAFETY: the path is NUL-terminated and all optional pointers are null;
    // the returned handle is checked before ownership is assumed.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            READ_CONTROL,
            SHARE_ALL,
            null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        // SAFETY: read immediately after the failed CreateFileW call.
        let error = unsafe { GetLastError() };
        return if matches!(error, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
            Err(StateError::Unavailable)
        } else {
            Err(StateError::UnsafePermissions)
        };
    }
    let handle = OwnedHandle(handle);
    validate_handle(handle.0, expected_directory)
}
fn validate_handle(handle: HANDLE, expected_directory: bool) -> Result<(), StateError> {
    // SAFETY: this plain Win32 output structure permits all-zero
    // initialization before the API fills every field.
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    // SAFETY: handle is owned by the caller's live guard and the output points
    // to writable initialized storage.
    if unsafe { GetFileInformationByHandle(handle, &mut information) } == 0 {
        return Err(StateError::Unavailable);
    }
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(StateError::UnsafePermissions);
    }
    let is_directory = information.dwFileAttributes & 0x10 != 0;
    if expected_directory != is_directory {
        return Err(StateError::UnsafePermissions);
    }

    let mut owner: PSID = null_mut();
    let mut dacl: *mut ACL = null_mut();
    let mut actual: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: handle is live; all output pointers refer to writable locals.
    // On success, owner and dacl point inside actual, whose guard stays live.
    let status = unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut actual,
        )
    };
    if status != 0 || actual.is_null() {
        return Err(StateError::Unavailable);
    }
    let actual = LocalAllocation(actual.cast());
    let current = CurrentUser::load()?;
    // SAFETY: owner is non-null within the guarded descriptor and current.sid
    // points inside current's aligned token-information buffer.
    if owner.is_null() || unsafe { EqualSid(owner, current.sid()) } == 0 {
        return Err(StateError::UnsafePermissions);
    }
    let mut control = 0u16;
    let mut revision = 0u32;
    // SAFETY: actual is a live descriptor returned by GetSecurityInfo and both
    // output pointers refer to writable scalar locals.
    if unsafe { GetSecurityDescriptorControl(actual.0.cast(), &mut control, &mut revision) } == 0
        || control & SE_DACL_PROTECTED == 0
    {
        return Err(StateError::UnsafePermissions);
    }
    let expected = SecurityDescriptor::for_sid(&current.sid_string()?, is_directory)?;
    let expected_dacl = expected.dacl()?;
    if !equal_acl(dacl, expected_dacl) {
        return Err(StateError::UnsafePermissions);
    }
    Ok(())
}
fn equal_acl(left: *const ACL, right: *const ACL) -> bool {
    if left.is_null() || right.is_null() {
        return false;
    }
    // SAFETY: both pointers originate in live Windows security descriptors.
    // IsValidAcl verifies their headers and ACE extents before slice creation.
    if unsafe { IsValidAcl(left) } == 0 || unsafe { IsValidAcl(right) } == 0 {
        return false;
    }
    // SAFETY: IsValidAcl accepted both ACLs, so their AclSize extents are
    // readable for the lifetime of their owning descriptors.
    let left_len = unsafe { (*left).AclSize as usize };
    // SAFETY: same validated-descriptor invariant as the left ACL.
    let right_len = unsafe { (*right).AclSize as usize };
    left_len == right_len
        // SAFETY: the validated sizes stay within both live ACL allocations.
        && unsafe { slice::from_raw_parts(left.cast::<u8>(), left_len) }
            // SAFETY: same validated-descriptor invariant as the left ACL.
            == unsafe { slice::from_raw_parts(right.cast::<u8>(), right_len) }
}

struct CurrentUser {
    buffer: Vec<usize>,
}

impl CurrentUser {
    fn load() -> Result<Self, StateError> {
        let mut token = null_mut();
        // SAFETY: the process pseudo-handle is always live and token points to
        // writable storage; a successful output becomes OwnedHandle below.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(StateError::Unavailable);
        }
        let token = OwnedHandle(token);
        let mut required = 0u32;
        // SAFETY: the live token is queried with the documented null-buffer
        // sizing call and required points to writable storage.
        unsafe { GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut required) };
        if required == 0 {
            return Err(StateError::Unavailable);
        }
        let words = (required as usize).div_ceil(size_of::<usize>());
        let mut buffer = vec![0usize; words];
        // SAFETY: Vec<usize> supplies suitable alignment and at least required
        // writable bytes; token and all pointers remain live during the call.
        if unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                required,
                &mut required,
            )
        } == 0
        {
            return Err(StateError::Unavailable);
        }
        Ok(Self { buffer })
    }

    fn sid(&self) -> PSID {
        // SAFETY: load allocated an aligned buffer of the exact returned size
        // and GetTokenInformation initialized its TOKEN_USER prefix.
        unsafe { (*(self.buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid }
    }

    fn sid_string(&self) -> Result<String, StateError> {
        let mut value = null_mut();
        // SAFETY: self owns the buffer containing the valid SID, and value is
        // writable. The successful allocation is guarded immediately below.
        if unsafe { ConvertSidToStringSidW(self.sid(), &mut value) } == 0 || value.is_null() {
            return Err(StateError::Unavailable);
        }
        let value = LocalAllocation(value.cast());
        let mut length = 0;
        // SAFETY: the Win32 contract returns a NUL-terminated UTF-16 LocalAlloc
        // string, kept alive by value while the terminator is located.
        while unsafe { *value.0.cast::<u16>().add(length) } != 0 {
            length += 1;
        }
        // SAFETY: length was measured within the live NUL-terminated buffer.
        String::from_utf16(unsafe { slice::from_raw_parts(value.0.cast::<u16>(), length) })
            .map_err(|_| StateError::Unavailable)
    }
}

struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

impl SecurityDescriptor {
    fn private(directory: bool) -> Result<Self, StateError> {
        let current = CurrentUser::load()?;
        Self::for_sid(&current.sid_string()?, directory)
    }

    fn for_sid(sid: &str, directory: bool) -> Result<Self, StateError> {
        let inherit = if directory { "OICI" } else { "" };
        let sddl = format!("D:P(A;{inherit};FA;;;{sid})");
        let wide = wide_string(&sddl)?;
        let mut descriptor = null_mut();
        // SAFETY: wide is NUL-terminated and descriptor points to writable
        // storage. A successful LocalAlloc result is guarded by Self.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        } == 0
            || descriptor.is_null()
        {
            return Err(StateError::Unavailable);
        }
        Ok(Self(descriptor))
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0.cast(),
            bInheritHandle: 0,
        }
    }

    fn dacl(&self) -> Result<*mut ACL, StateError> {
        let mut present = 0;
        let mut defaulted = 0;
        let mut dacl = null_mut();
        // SAFETY: self owns a live descriptor and all output pointers refer to
        // writable locals; the returned DACL cannot outlive self.
        if unsafe { GetSecurityDescriptorDacl(self.0, &mut present, &mut dacl, &mut defaulted) }
            == 0
            || present == 0
            || dacl.is_null()
        {
            return Err(StateError::Unavailable);
        }
        Ok(dacl)
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        // SAFETY: constructors admit only non-null LocalAlloc results and this
        // guard is their sole owner.
        unsafe { LocalFree(self.0.cast()) };
    }
}

struct LocalAllocation(*mut c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: every construction site transfers one non-null LocalAlloc
        // result into this sole owner.
        unsafe { LocalFree(self.0) };
    }
}

struct OwnedHandle(HANDLE);

impl OwnedHandle {
    fn into_raw(self) -> HANDLE {
        let raw = self.0;
        std::mem::forget(self);
        raw
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: every construction site checks for failure first and each
        // valid handle is closed exactly once unless transferred by into_raw.
        unsafe { CloseHandle(self.0) };
    }
}

fn wide_path(path: &Path) -> Result<Vec<u16>, StateError> {
    let absolute = std::path::absolute(path).map_err(|_| StateError::Unavailable)?;
    let encoded = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
    if encoded.contains(&0) {
        return Err(StateError::Unavailable);
    }
    let mut output =
        if encoded.starts_with(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16])
            || encoded.starts_with(&[b'\\' as u16, b'\\' as u16, b'.' as u16, b'\\' as u16])
        {
            encoded
        } else if encoded.starts_with(&[b'\\' as u16, b'\\' as u16]) {
            "\\\\?\\UNC\\"
                .encode_utf16()
                .chain(encoded.into_iter().skip(2))
                .collect()
        } else {
            "\\\\?\\".encode_utf16().chain(encoded).collect()
        };
    output.push(0);
    Ok(output)
}

fn wide_string(value: &str) -> Result<Vec<u16>, StateError> {
    if value.contains('\0') {
        return Err(StateError::Unavailable);
    }
    Ok(value.encode_utf16().chain([0]).collect())
}
