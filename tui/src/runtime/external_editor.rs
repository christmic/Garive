use std::{
    env,
    ffi::OsStr,
    fs::OpenOptions,
    io::{Read, Write},
    process::{ExitStatus, Stdio},
};

use tempfile::TempPath;
use tokio::process::{Child, Command};

use crate::input::EditorState;

use super::app::RuntimeState;

const MAX_COMMAND_BYTES: usize = 4_096;
const MAX_ARGUMENTS: usize = 32;
const MAX_DRAFT_BYTES: usize = 4_096;
const MAX_SAVED_BYTES: usize = MAX_DRAFT_BYTES + 2;

pub(super) const MISSING_EDITOR: &str = "Set VISUAL or EDITOR before editing externally.";
pub(super) const INVALID_EDITOR: &str = "The configured external editor command is invalid.";
pub(super) const PREPARE_FAILED: &str =
    "The external editor could not be prepared; the draft was kept.";
pub(super) const SPAWN_FAILED: &str = "The external editor could not start; the draft was kept.";
pub(super) const EXIT_FAILED: &str =
    "The external editor exited unsuccessfully; the draft was kept.";
pub(super) const READ_FAILED: &str =
    "The external editor result could not be read; the draft was kept.";
pub(super) const INVALID_UTF8: &str =
    "The external editor saved invalid UTF-8; the draft was kept.";
pub(super) const TOO_LARGE: &str =
    "The external editor result exceeds the prompt limit; the draft was kept.";
pub(super) const STALE_DRAFT: &str =
    "The draft changed while the external editor was open; the newer draft was kept.";

pub(super) struct EditorRequest {
    pub(super) session_id: Option<String>,
    original_text: String,
}

impl std::fmt::Debug for EditorRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditorRequest")
            .field("session_selected", &self.session_id.is_some())
            .field("draft_bytes", &self.original_text.len())
            .finish()
    }
}

impl EditorRequest {
    pub(super) fn new(session_id: Option<String>, original_text: String) -> Self {
        Self {
            session_id,
            original_text,
        }
    }

    pub(super) fn original_text(&self) -> &str {
        &self.original_text
    }
}

pub(super) fn request(state: &mut RuntimeState) {
    if state.external_editor_request.is_some() {
        return;
    }
    if state.composer_is_frozen() {
        state.explain_frozen_composer();
        return;
    }
    state.external_editor_request = Some(EditorRequest::new(
        state.model.selected_session.clone(),
        state.model.composer.text().to_owned(),
    ));
}

pub(super) fn apply(
    state: &mut RuntimeState,
    request: EditorRequest,
    result: Result<String, &'static str>,
) {
    if state.model.selected_session != request.session_id
        || state.model.composer.text() != request.original_text()
    {
        state.model.notice = Some(STALE_DRAFT.into());
        return;
    }
    state.model.notice = Some(apply_result(&mut state.model.composer, result));
    state.model.prompt_history_browser.reset();
    state.model.command_suggestion_dismissed = None;
    state.model.command_suggestion_selection = 0;
    state.force_redraw = true;
}

fn apply_result(editor: &mut EditorState, result: Result<String, &'static str>) -> String {
    match result {
        Ok(text) => match editor.replace_undoable(&text) {
            Ok(()) => "External edit applied to the draft.".into(),
            Err(_) => {
                "The external editor result contains unsupported input; the draft was kept.".into()
            }
        },
        Err(message) => message.into(),
    }
}

pub(super) struct PreparedEditor {
    request: EditorRequest,
    argv: Vec<String>,
    file: PromptFile,
}

impl PreparedEditor {
    pub(super) fn spawn(&self) -> std::io::Result<Child> {
        let mut command = Command::new(&self.argv[0]);
        command
            .args(&self.argv[1..])
            .arg(self.file.path.as_os_str())
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        command.spawn()
    }

    pub(super) fn finish(
        self,
        status: ExitStatus,
    ) -> (EditorRequest, Result<String, &'static str>) {
        let result = if status.success() {
            self.file.read(self.request.original_text())
        } else {
            Err(EXIT_FAILED)
        };
        (self.request, result)
    }

    pub(super) fn failed(
        self,
        message: &'static str,
    ) -> (EditorRequest, Result<String, &'static str>) {
        (self.request, Err(message))
    }
}

struct PromptFile {
    path: TempPath,
}

impl PromptFile {
    fn create(seed: &str) -> Result<Self, &'static str> {
        let path = env::temp_dir().join(format!("garive-prompt-{}.md", uuid::Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).map_err(|_| PREPARE_FAILED)?;
        file.write_all(seed.as_bytes())
            .and_then(|_| file.flush())
            .map_err(|_| PREPARE_FAILED)?;
        let path = TempPath::try_from_path(path).map_err(|_| PREPARE_FAILED)?;
        Ok(Self { path })
    }

    fn read(&self, original: &str) -> Result<String, &'static str> {
        let mut file = OpenOptions::new()
            .read(true)
            .open(&self.path)
            .map_err(|_| READ_FAILED)?;
        if file.metadata().map_err(|_| READ_FAILED)?.len() > MAX_SAVED_BYTES as u64 {
            return Err(TOO_LARGE);
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take((MAX_SAVED_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| READ_FAILED)?;
        if bytes.len() > MAX_SAVED_BYTES {
            return Err(TOO_LARGE);
        }
        let mut text = String::from_utf8(bytes).map_err(|_| INVALID_UTF8)?;
        text = text.replace("\r\n", "\n");
        if !original.ends_with('\n') && text.ends_with('\n') {
            text.pop();
        }
        if text.len() > MAX_DRAFT_BYTES {
            return Err(TOO_LARGE);
        }
        Ok(text)
    }
}

pub(super) fn prepare(
    request: EditorRequest,
) -> Result<PreparedEditor, (&'static str, EditorRequest)> {
    let visual = env::var("VISUAL").ok();
    let editor = env::var("EDITOR").ok();
    prepare_with(request, visual.as_deref(), editor.as_deref())
}

fn prepare_with(
    request: EditorRequest,
    visual: Option<&str>,
    editor: Option<&str>,
) -> Result<PreparedEditor, (&'static str, EditorRequest)> {
    let command = visual
        .filter(|value| !value.trim().is_empty())
        .or_else(|| editor.filter(|value| !value.trim().is_empty()));
    let Some(command) = command else {
        return Err((MISSING_EDITOR, request));
    };
    if command.len() > MAX_COMMAND_BYTES || command.contains('\0') {
        return Err((INVALID_EDITOR, request));
    }
    let Some(argv) = shlex::split(command) else {
        return Err((INVALID_EDITOR, request));
    };
    if argv.is_empty()
        || argv.len() > MAX_ARGUMENTS
        || argv
            .first()
            .is_none_or(|program| OsStr::new(program).is_empty())
    {
        return Err((INVALID_EDITOR, request));
    }
    let file = match PromptFile::create(request.original_text()) {
        Ok(file) => file,
        Err(message) => return Err((message, request)),
    };
    Ok(PreparedEditor {
        request,
        argv,
        file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_precedes_editor_and_quoted_arguments_stay_structured() {
        let prepared = prepare_with(
            EditorRequest::new(None, "draft".into()),
            Some("code --wait \"profile name\""),
            Some("ignored"),
        )
        .unwrap();
        assert_eq!(prepared.argv, ["code", "--wait", "profile name"]);
    }

    #[test]
    fn invalid_or_missing_commands_never_create_a_child() {
        for (visual, expected) in [
            (None, MISSING_EDITOR),
            (Some("\"unterminated"), INVALID_EDITOR),
            (Some(""), MISSING_EDITOR),
        ] {
            let error = prepare_with(EditorRequest::new(None, String::new()), visual, None)
                .err()
                .unwrap();
            assert_eq!(error.0, expected);
        }
    }

    #[tokio::test]
    async fn command_and_argument_bounds_are_exact_and_spawn_failure_is_safe() {
        let too_many = std::iter::repeat_n("x", MAX_ARGUMENTS + 1)
            .collect::<Vec<_>>()
            .join(" ");
        for command in [
            "x\0y".to_owned(),
            "x".repeat(MAX_COMMAND_BYTES + 1),
            too_many,
        ] {
            let error = prepare_with(
                EditorRequest::new(None, String::new()),
                Some(&command),
                None,
            )
            .err()
            .unwrap();
            assert_eq!(error.0, INVALID_EDITOR);
        }
        assert!(prepare_with(
            EditorRequest::new(None, String::new()),
            Some(&"x".repeat(MAX_COMMAND_BYTES)),
            None,
        )
        .is_ok());
        assert!(prepare_with(
            EditorRequest::new(None, String::new()),
            Some(
                &std::iter::repeat_n("x", MAX_ARGUMENTS)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            None,
        )
        .is_ok());

        let prepared = prepare_with(
            EditorRequest::new(None, "private".into()),
            Some("/garive/no/such/editor"),
            None,
        )
        .unwrap();
        assert!(prepared.spawn().is_err());
    }

    #[test]
    fn private_file_normalizes_one_editor_newline_and_is_removed_on_drop() {
        let prepared = prepare_with(
            EditorRequest::new(Some("session".into()), "seed".into()),
            Some("editor"),
            None,
        )
        .unwrap();
        let path = prepared.file.path.to_path_buf();
        std::fs::write(&path, "edited\r\n").unwrap();
        assert_eq!(prepared.file.read("seed").unwrap(), "edited");
        drop(prepared);
        assert!(!path.exists());
    }

    #[test]
    fn oversized_and_invalid_utf8_results_are_rejected() {
        let prepared = prepare_with(
            EditorRequest::new(None, String::new()),
            Some("editor"),
            None,
        )
        .unwrap();
        std::fs::write(&prepared.file.path, vec![b'x'; MAX_SAVED_BYTES + 1]).unwrap();
        assert_eq!(prepared.file.read("").unwrap_err(), TOO_LARGE);
        std::fs::write(&prepared.file.path, [0xff]).unwrap();
        assert_eq!(prepared.file.read("").unwrap_err(), INVALID_UTF8);
    }

    #[test]
    fn unsafe_external_content_never_replaces_the_draft() {
        let mut editor = EditorState::new(MAX_DRAFT_BYTES);
        editor.insert("kept").unwrap();
        let notice = apply_result(&mut editor, Ok("hidden\u{1b}[31m".into()));
        assert_eq!(editor.text(), "kept");
        assert!(notice.contains("unsupported input"));
    }
}
