use std::{fs, path::Path};

const MAX_PRODUCTION_LINES: usize = 500;

#[test]
fn production_modules_remain_bounded() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut rust_files = Vec::new();
    collect_rust_files(&source, &mut rust_files);
    rust_files.sort();

    let oversized = rust_files
        .into_iter()
        .filter_map(|path| {
            let contents = fs::read_to_string(&path).expect("production source is readable");
            let lines = contents.lines().count();
            (lines > MAX_PRODUCTION_LINES).then(|| {
                format!(
                    "{} has {lines} lines (limit {MAX_PRODUCTION_LINES})",
                    path.strip_prefix(&source)
                        .expect("source file is beneath src")
                        .display()
                )
            })
        })
        .collect::<Vec<_>>();

    assert!(
        oversized.is_empty(),
        "TUI production modules exceeded the architecture bound:\n{}",
        oversized.join("\n")
    );
}

#[test]
fn session_pagination_uses_one_typed_runtime_boundary_in_all_terminal_modes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let state = fs::read_to_string(root.join("src/runtime/app/state.rs"))
        .expect("runtime state is readable");
    let screen_reader = fs::read_to_string(root.join("src/runtime/app/screen_reader.rs"))
        .expect("screen-reader runtime is readable");
    let full_screen =
        fs::read_to_string(root.join("src/runtime/app.rs")).expect("runtime is readable");

    assert!(state.contains("AppAction::LoadSessionPageRequested"));
    assert!(!state.contains("host::load_session_page"));
    assert!(screen_reader.contains("controller::handle_terminal"));
    assert!(full_screen.contains("controller::handle_terminal"));
    assert!(!screen_reader.contains("list_sessions("));
    assert!(!screen_reader.contains("LoadSessionPageRequested"));
}

#[test]
fn terminal_modes_share_the_executable_fair_scheduler() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let full_screen =
        fs::read_to_string(root.join("src/runtime/app.rs")).expect("runtime is readable");
    let screen_reader = fs::read_to_string(root.join("src/runtime/app/screen_reader.rs"))
        .expect("screen-reader runtime is readable");

    for source in [&full_screen, &screen_reader] {
        assert!(source.contains("scheduling::select_next("));
        assert!(source.contains("ResizeCoalescer::default()"));
        assert!(!source.contains("message = receiver.recv()"));
    }
}

fn collect_rust_files(directory: &Path, output: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(directory).expect("source directory is readable") {
        let path = entry.expect("source entry is readable").path();
        if path.is_dir() {
            collect_rust_files(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push(path);
        }
    }
}
