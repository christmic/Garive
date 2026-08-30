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
