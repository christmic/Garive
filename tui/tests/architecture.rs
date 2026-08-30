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
