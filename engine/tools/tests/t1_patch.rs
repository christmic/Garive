use garive_tools::{apply_t1_patch, t1_patch_targets, T1PatchError};

#[test]
fn applies_ordered_unique_context_hunks_and_preserves_newline() {
    let patch = "*** Begin Patch\n*** Update File: src/lib.rs\n@@\n alpha\n-beta\n+BETA\n@@\n gamma\n+delta\n*** End Patch";
    assert_eq!(
        t1_patch_targets(patch)
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
        ["src/lib.rs"]
    );
    assert_eq!(
        apply_t1_patch(patch, "src/lib.rs", "alpha\nbeta\ngamma\n").unwrap(),
        "alpha\nBETA\ngamma\ndelta\n"
    );
}

#[test]
fn rejects_ambiguous_missing_and_unanchored_hunks() {
    let ambiguous = "*** Begin Patch\n*** Update File: f\n@@\n-same\n+new\n*** End Patch";
    assert_eq!(
        apply_t1_patch(ambiguous, "f", "same\nsame\n"),
        Err(T1PatchError::ContextMismatch)
    );
    assert_eq!(
        apply_t1_patch(ambiguous, "missing", "same\n"),
        Err(T1PatchError::TargetMissing)
    );
    let unanchored = "*** Begin Patch\n*** Update File: f\n@@\n+new\n*** End Patch";
    assert_eq!(
        t1_patch_targets(unanchored),
        Err(T1PatchError::InvalidSyntax)
    );
}

#[test]
fn no_newline_marker_is_only_valid_on_the_final_affected_line() {
    let patch = "*** Begin Patch\n*** Update File: f\n@@\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n*** End Patch";
    assert_eq!(apply_t1_patch(patch, "f", "old").unwrap(), "new");
    assert_eq!(
        apply_t1_patch(patch, "f", "old\n"),
        Err(T1PatchError::ContextMismatch)
    );
}

#[test]
fn accepts_standard_unified_diff_for_existing_files() {
    let patch = "--- a/source.txt\n+++ b/source.txt\n@@ -1 +1 @@\n-ORIGINAL_ALPHA\n+EDITED_BETA\n";
    assert_eq!(
        t1_patch_targets(patch)
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
        ["source.txt"]
    );
    assert_eq!(
        apply_t1_patch(patch, "source.txt", "ORIGINAL_ALPHA\n").unwrap(),
        "EDITED_BETA\n"
    );
}

#[test]
fn unified_diff_rejects_rename_and_create_headers() {
    let rename = "--- a/old.txt\n+++ b/new.txt\n@@ -1 +1 @@\n-old\n+new\n";
    assert_eq!(t1_patch_targets(rename), Err(T1PatchError::InvalidSyntax));
    let create = "--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+new\n";
    assert_eq!(t1_patch_targets(create), Err(T1PatchError::InvalidSyntax));
}
