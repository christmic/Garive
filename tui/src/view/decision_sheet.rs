use crate::{
    application::{ActionOverlayBinding, AppModel, Overlay},
    input::{response_schema_control, SchemaControl},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::presentation::{action_overlay_copy, suspension_copy};

pub(crate) struct DecisionSheetSpec {
    pub(crate) title: String,
    pub(crate) body: Vec<String>,
    pub(crate) response: Option<DecisionResponseSpec>,
    pub(crate) tone: DecisionSheetTone,
    pub(crate) actions: Vec<ActionOverlayBinding>,
}

pub(crate) enum DecisionResponseSpec {
    Editor {
        guidance: &'static str,
        draft: String,
        cursor: usize,
    },
    Choices {
        guidance: &'static str,
        choices: Vec<String>,
        selected: usize,
    },
    ReadOnly {
        guidance: &'static str,
    },
}

#[derive(Clone, Copy)]
pub(crate) enum DecisionSheetTone {
    Neutral,
    Warning,
    Danger,
}

#[derive(Clone)]
pub(crate) enum DecisionRow {
    Body {
        value: String,
        tone: Option<DecisionSheetTone>,
    },
    Editor {
        before: String,
        after: String,
        empty: bool,
    },
    Choice {
        index: usize,
        value: String,
        selected: bool,
    },
    Blank,
    Label(&'static str),
    Guidance(String),
    Actions(Vec<ActionOverlayBinding>),
}

pub(crate) struct DecisionLayout {
    pub(crate) rows: Vec<DecisionRow>,
}

pub(crate) fn layout(spec: &DecisionSheetSpec, width: u16, height: usize) -> DecisionLayout {
    let mut body = body_rows(spec, width)
        .into_iter()
        .enumerate()
        .map(|(index, value)| DecisionRow::Body {
            value,
            tone: (index == 0 && !matches!(spec.tone, DecisionSheetTone::Neutral))
                .then_some(spec.tone),
        })
        .collect::<Vec<_>>();
    let mut response = Vec::new();
    let mut primary = None;
    if let Some(control) = spec.response.as_ref() {
        let (label, guidance) = match control {
            DecisionResponseSpec::Editor {
                guidance,
                draft,
                cursor,
            } => {
                let (before, after) =
                    editor_view(draft, *cursor, usize::from(width.saturating_sub(2)));
                primary = Some(DecisionRow::Editor {
                    before,
                    after,
                    empty: draft.is_empty(),
                });
                ("Response", *guidance)
            }
            DecisionResponseSpec::Choices {
                guidance,
                choices,
                selected,
            } => {
                response.extend(choices.iter().enumerate().map(|(index, choice)| {
                    DecisionRow::Choice {
                        index,
                        value: truncate_display(
                            &super::safe_text(choice),
                            usize::from(width.saturating_sub(3)),
                        ),
                        selected: index == *selected,
                    }
                }));
                primary = choices.get(*selected).map(|choice| DecisionRow::Choice {
                    index: *selected,
                    value: truncate_display(
                        &super::safe_text(choice),
                        usize::from(width.saturating_sub(3)),
                    ),
                    selected: true,
                });
                ("Choose", *guidance)
            }
            DecisionResponseSpec::ReadOnly { guidance } => ("Read only", *guidance),
        };
        response.insert(0, DecisionRow::Label(label));
        if let Some(editor) = primary.clone().filter(|_| response.len() == 1) {
            response.push(editor);
        }
        response.extend(
            display_rows(guidance, width)
                .into_iter()
                .map(DecisionRow::Guidance),
        );
    }
    if !body.is_empty() && !response.is_empty() {
        body.push(DecisionRow::Blank);
    }
    body.extend(response);
    let actions = action_groups(&spec.actions, width)
        .into_iter()
        .map(DecisionRow::Actions)
        .collect::<Vec<_>>();
    let fits = body.len().saturating_add(actions.len()).saturating_add(1) <= height;
    let mut rows = if fits {
        body.push(DecisionRow::Blank);
        body
    } else {
        let mut compact = body.into_iter().take(1).collect::<Vec<_>>();
        if let Some(primary) = primary {
            compact.push(primary);
        }
        compact.truncate(height.saturating_sub(actions.len()));
        compact
    };
    if height != usize::MAX {
        while rows.len().saturating_add(actions.len()) < height {
            rows.push(DecisionRow::Blank);
        }
    }
    rows.extend(actions);
    DecisionLayout { rows }
}

pub(crate) fn project(model: &AppModel, overlay: Overlay) -> Option<DecisionSheetSpec> {
    if overlay == Overlay::Suspension {
        return Some(suspension(model));
    }
    let copy = action_overlay_copy(model, overlay)?;
    Some(DecisionSheetSpec {
        title: copy.title.into(),
        body: copy.body.lines().map(str::to_owned).collect(),
        response: None,
        tone: match overlay {
            Overlay::UnknownCommand | Overlay::AbandonConfirmation => DecisionSheetTone::Danger,
            Overlay::EphemeralConfirmation => DecisionSheetTone::Warning,
            _ => DecisionSheetTone::Neutral,
        },
        actions: model.decision_bindings(overlay)?.to_vec(),
    })
}

pub(crate) fn action_groups(
    actions: &[ActionOverlayBinding],
    width: u16,
) -> Vec<Vec<ActionOverlayBinding>> {
    let mut groups = Vec::<Vec<ActionOverlayBinding>>::new();
    for action in actions {
        let item_width = action.visual_key.width() + action.action.width() + 3;
        let used = groups.last().map_or(0, |group| {
            group
                .iter()
                .map(|item| item.visual_key.width() + item.action.width() + 3)
                .sum::<usize>()
                + group.len().saturating_sub(1) * 2
        });
        if groups.is_empty()
            || groups.last().is_some_and(|group| {
                !group.is_empty() && used + 2 + item_width > usize::from(width)
            })
        {
            groups.push(Vec::new());
        }
        groups
            .last_mut()
            .expect("action group exists")
            .push(*action);
    }
    groups
}

fn truncate_display(value: &str, width: usize) -> String {
    value
        .graphemes(true)
        .scan(0usize, |used, grapheme| {
            *used = used.saturating_add(grapheme.width());
            (*used <= width).then_some(grapheme)
        })
        .collect()
}

fn editor_view(draft: &str, cursor: usize, width: usize) -> (String, String) {
    let graphemes = draft.graphemes(true).collect::<Vec<_>>();
    let cursor = cursor.min(graphemes.len());
    let budget = width.saturating_sub(1);
    let mut start = cursor;
    let mut before = 0usize;
    while start > 0 {
        let candidate = graphemes[start - 1].width();
        if before.saturating_add(candidate) > budget / 2 {
            break;
        }
        start -= 1;
        before += candidate;
    }
    let mut end = cursor;
    let mut used = before.saturating_add(1);
    while end < graphemes.len() {
        let candidate = graphemes[end].width();
        if used.saturating_add(candidate) > width {
            break;
        }
        used += candidate;
        end += 1;
    }
    while start > 0 && used < width {
        let candidate = graphemes[start - 1].width();
        if used.saturating_add(candidate) > width {
            break;
        }
        start -= 1;
        used += candidate;
    }
    (
        format!("› {}", graphemes[start..cursor].concat()),
        graphemes[cursor..end].concat(),
    )
}

pub(crate) fn display_rows(value: &str, width: u16) -> Vec<String> {
    let width = usize::from(width.max(1));
    let mut rows = Vec::new();
    for logical in value.split('\n') {
        let mut current = Vec::<&str>::new();
        for grapheme in logical.graphemes(true) {
            let used = current.iter().map(|part| part.width()).sum::<usize>();
            if used > 0 && used.saturating_add(grapheme.width()) > width {
                if grapheme.trim().is_empty() {
                    rows.push(current.concat());
                    current.clear();
                    continue;
                }
                if let Some(space) = current.iter().rposition(|part| part.trim().is_empty()) {
                    rows.push(current[..space].concat());
                    current = current[space + 1..].to_vec();
                } else {
                    rows.push(current.concat());
                    current.clear();
                }
            }
            if !current.is_empty() || !grapheme.trim().is_empty() {
                current.push(grapheme);
            }
        }
        rows.push(current.concat());
    }
    if rows.is_empty() {
        vec![String::new()]
    } else {
        rows
    }
}

pub(crate) fn body_rows(spec: &DecisionSheetSpec, width: u16) -> Vec<String> {
    let mut rows = Vec::new();
    for (index, value) in spec.body.iter().enumerate() {
        let width = if index == 0 && !matches!(spec.tone, DecisionSheetTone::Neutral) {
            width.saturating_sub(3).max(1)
        } else {
            width
        };
        rows.extend(display_rows(&super::safe_text(value), width));
    }
    rows
}

fn suspension(model: &AppModel) -> DecisionSheetSpec {
    let copy = suspension_copy(model.suspension.as_ref());
    let mut body = vec![copy.context.into()];
    if let Some(message) = copy.message {
        body.push(message);
    }
    DecisionSheetSpec {
        title: copy.title.into(),
        body,
        response: Some(
            if let Some(control) = model
                .suspension_is_interactive()
                .then(|| {
                    model.suspension.as_ref().and_then(|suspension| {
                        suspension
                            .response_schema_json
                            .as_deref()
                            .and_then(response_schema_control)
                    })
                })
                .flatten()
            {
                match control {
                    SchemaControl::Editor => DecisionResponseSpec::Editor {
                        guidance: copy.guidance,
                        draft: model
                            .suspension_response
                            .as_ref()
                            .map(|state| state.editor.text().to_owned())
                            .unwrap_or_default(),
                        cursor: model
                            .suspension_response
                            .as_ref()
                            .map_or(0, |state| state.editor.cursor_grapheme()),
                    },
                    SchemaControl::Choices(choices) => DecisionResponseSpec::Choices {
                        guidance: copy.guidance,
                        selected: model
                            .suspension_response
                            .as_ref()
                            .map_or(0, |state| state.choice_selection),
                        choices,
                    },
                }
            } else {
                DecisionResponseSpec::ReadOnly {
                    guidance: "This suspension is status-only; no response can be submitted here.",
                }
            },
        ),
        tone: DecisionSheetTone::Warning,
        actions: model
            .decision_bindings(Overlay::Suspension)
            .unwrap_or_default()
            .to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toned_body_wraps_with_marker_budget_and_preserves_word_boundaries() {
        let spec = DecisionSheetSpec {
            title: "Decision".into(),
            body: vec!["Review Host truth safely 界界界界界界界界".into()],
            response: None,
            tone: DecisionSheetTone::Danger,
            actions: Vec::new(),
        };
        let rows = body_rows(&spec, 20);
        assert_eq!(rows[0], "Review Host truth");
        assert!(rows.iter().all(|row| row.width() <= 17));
        assert!(rows.iter().any(|row| row.contains('界')));
    }

    #[test]
    fn compact_layout_keeps_selected_choice_and_all_actions() {
        use crate::application::{ActionOverlayIntent, ActionOverlayKey};

        let action = |key, visual_key, intent| ActionOverlayBinding {
            key,
            visual_key,
            spoken_key: visual_key,
            action: "act safely",
            intent,
        };
        let spec = DecisionSheetSpec {
            title: "Decision".into(),
            body: vec!["A long consequence that cannot fit compact height.".into()],
            response: Some(DecisionResponseSpec::Choices {
                guidance: "Choose one.",
                choices: vec!["first".into(), "second".into()],
                selected: 1,
            }),
            tone: DecisionSheetTone::Warning,
            actions: vec![
                action(
                    ActionOverlayKey::Enter,
                    "Enter",
                    ActionOverlayIntent::SubmitSuspension,
                ),
                action(
                    ActionOverlayKey::CtrlQ,
                    "Ctrl+Q",
                    ActionOverlayIntent::LeaveSafely,
                ),
            ],
        };
        let layout = layout(&spec, 34, 4);
        assert_eq!(layout.rows.len(), 4);
        assert!(layout.rows.iter().any(|row| matches!(
            row,
            DecisionRow::Choice {
                index: 1,
                selected: true,
                ..
            }
        )));
        assert_eq!(
            layout
                .rows
                .iter()
                .filter(|row| matches!(row, DecisionRow::Actions(_)))
                .count(),
            2
        );
    }
}
