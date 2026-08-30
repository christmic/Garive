use std::fs;

use garive_desktop::{
    DesktopProductStore, DesktopProductStoreError, MAX_PRODUCT_STORE_BYTES,
    MAX_UPDATE_PENDING_BYTES,
};
use tempfile::tempdir;

#[test]
fn preferences_and_pending_are_bounded_atomic_and_separate() {
    let directory = tempdir().unwrap();
    let root = directory.path().join("product");
    let store = DesktopProductStore::new(&root).unwrap();
    assert_eq!(store.read_preferences().unwrap(), None);
    assert_eq!(store.read_pending().unwrap(), None);
    assert_eq!(store.read_update_pending().unwrap(), None);

    store.write_preferences(br#"{"schema_version":1}"#).unwrap();
    store
        .write_pending(Some(br#"{"schema_version":1,"status":"unknown"}"#))
        .unwrap();
    store
        .write_update_pending(Some(
            br#"{"schema_version":1,"current_version":"1.0.0","target_version":"1.1.0","phase":"installing"}"#,
        ))
        .unwrap();
    assert_eq!(
        store.read_preferences().unwrap().unwrap(),
        br#"{"schema_version":1}"#
    );
    assert!(store
        .read_pending()
        .unwrap()
        .unwrap()
        .ends_with(br#""unknown"}"#));
    assert!(store
        .read_update_pending()
        .unwrap()
        .unwrap()
        .ends_with(br#""installing"}"#));
    store.write_pending(None).unwrap();
    assert_eq!(store.read_pending().unwrap(), None);
    assert!(store.read_update_pending().unwrap().is_some());
    store.write_update_pending(None).unwrap();
    assert_eq!(store.read_update_pending().unwrap(), None);
    assert_eq!(
        store.read_preferences().unwrap().unwrap(),
        br#"{"schema_version":1}"#
    );
    assert!(fs::read_dir(root).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".pending")));
}

#[test]
fn invalid_roots_empty_documents_and_oversized_files_fail_closed() {
    assert!(matches!(
        DesktopProductStore::new(""),
        Err(DesktopProductStoreError::InvalidValue)
    ));
    let directory = tempdir().unwrap();
    let root = directory.path().join("product");
    let store = DesktopProductStore::new(&root).unwrap();
    assert_eq!(
        store.write_preferences(&[]),
        Err(DesktopProductStoreError::InvalidValue)
    );
    assert_eq!(
        store.write_pending(Some(&vec![b'x'; MAX_PRODUCT_STORE_BYTES + 1])),
        Err(DesktopProductStoreError::InvalidValue)
    );
    assert_eq!(
        store.write_update_pending(Some(&vec![b'x'; MAX_UPDATE_PENDING_BYTES + 1])),
        Err(DesktopProductStoreError::InvalidValue)
    );
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("client-preferences-v1.json"),
        vec![b'x'; MAX_PRODUCT_STORE_BYTES + 1],
    )
    .unwrap();
    assert_eq!(
        store.read_preferences(),
        Err(DesktopProductStoreError::InvalidValue)
    );
}
