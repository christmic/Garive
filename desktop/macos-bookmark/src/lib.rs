//! Narrow safe wrapper around macOS security-scoped URL bookmarks.

#![deny(missing_docs)]

use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    ptr::NonNull,
};

use objc2::rc::Retained;
use objc2_foundation::{
    NSData, NSHomeDirectory, NSString, NSURLBookmarkCreationOptions,
    NSURLBookmarkResolutionOptions, NSURL,
};

/// A stable native bookmark operation failure without path or Foundation details.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookmarkError {
    /// The supplied path cannot be represented as a native file URL.
    InvalidPath,
    /// Foundation rejected bookmark creation or resolution.
    NativeFailure,
    /// Security-scoped access could not be activated.
    AccessDenied,
}

/// A resolved bookmark whose security-scoped access remains active until drop.
pub struct ScopedBookmark {
    url: Retained<NSURL>,
    path: PathBuf,
}

impl ScopedBookmark {
    /// Returns the resolved filesystem path for backend-private use.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScopedBookmark {
    fn drop(&mut self) {
        // SAFETY: this balances the successful start call made by `resolve` on
        // the same retained NSURL, before that object is released.
        unsafe { self.url.stopAccessingSecurityScopedResource() };
    }
}

/// Creates a read-only security-scoped bookmark for an already-authorized directory.
pub fn create_read_only(path: &Path) -> Result<Vec<u8>, BookmarkError> {
    let path = path.to_str().ok_or(BookmarkError::InvalidPath)?;
    let value = NSString::from_str(path);
    let url = NSURL::fileURLWithPath_isDirectory(&value, true);
    let options = NSURLBookmarkCreationOptions::WithSecurityScope
        | NSURLBookmarkCreationOptions::SecurityScopeAllowOnlyReadAccess;
    let data = url
        .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
            options, None, None,
        )
        .map_err(|_| BookmarkError::NativeFailure)?;
    copy_data(&data)
}

/// Returns the operating-system home directory for broad-root rejection.
pub fn home_directory() -> PathBuf {
    PathBuf::from(NSHomeDirectory().to_string())
}

/// Resolves bookmark bytes without UI and starts balanced security-scoped access.
pub fn resolve(bytes: &[u8]) -> Result<(ScopedBookmark, bool), BookmarkError> {
    if bytes.is_empty() {
        return Err(BookmarkError::NativeFailure);
    }
    // SAFETY: Foundation copies exactly `bytes.len()` initialized bytes before
    // this borrowed slice can expire.
    let data = unsafe { NSData::dataWithBytes_length(bytes.as_ptr().cast(), bytes.len()) };
    let mut stale = objc2::runtime::Bool::NO;
    // SAFETY: `stale` is a valid out-pointer for the duration of this call.
    let url = unsafe {
        NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
            &data,
            NSURLBookmarkResolutionOptions::WithoutUI
                | NSURLBookmarkResolutionOptions::WithSecurityScope,
            None,
            &mut stale,
        )
    }
    .map_err(|_| BookmarkError::NativeFailure)?;
    // SAFETY: the retained NSURL remains alive in `ScopedBookmark`; Drop
    // balances this call only after a successful start.
    if !unsafe { url.startAccessingSecurityScopedResource() } {
        return Err(BookmarkError::AccessDenied);
    }
    let path = url
        .path()
        .map(|value| PathBuf::from(value.to_string()))
        .ok_or(BookmarkError::NativeFailure)?;
    Ok((ScopedBookmark { url, path }, stale.as_bool()))
}

fn copy_data(data: &NSData) -> Result<Vec<u8>, BookmarkError> {
    let length = data.length();
    let mut bytes = vec![0_u8; length];
    if length > 0 {
        let destination = NonNull::new(bytes.as_mut_ptr().cast::<c_void>())
            .ok_or(BookmarkError::NativeFailure)?;
        // SAFETY: `bytes` owns at least `length` writable bytes and NSData
        // guarantees its readable length is exactly the value queried above.
        unsafe { data.getBytes_length(destination, length) };
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_bookmark_round_trips_without_embedding_the_plain_path() {
        let directory = tempfile::tempdir().unwrap();
        let bytes = create_read_only(directory.path()).unwrap();
        assert!(!bytes.is_empty());
        assert!(
            !String::from_utf8_lossy(&bytes).contains(directory.path().to_string_lossy().as_ref())
        );
        let (resolved, stale) = resolve(&bytes).unwrap();
        assert!(!stale);
        assert_eq!(resolved.path(), directory.path().canonicalize().unwrap());
    }
}
