use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget, Wrap},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    application::{AppModel, Overlay},
    input::{help_hints, COMMAND_PALETTE},
    Theme,
};

use super::{
    decision_sheet, inspector, palette,
    presentation::HELP_NOTES,
    primitives::{key_hints, truncate_display},
    safe_text,
    session::picker_line,
    style::Palette,
};

pub(super) mod geometry;

use geometry::{overlay_geometry, overlay_padding};

struct OverlaySpec {
    title: String,
    content: Text<'static>,
}

pub(super) fn render_overlay(
    model: &AppModel,
    overlay: Overlay,
    theme: Theme,
    area: Rect,
    buffer: &mut Buffer,
) {
    let colors = palette(theme);
    let geometry = overlay_geometry(model, overlay, area);
    let popup = geometry.popup;
    buffer.set_style(area, colors.modal_backdrop);
    let halo = modal_halo(popup, area);
    Clear.render(halo, buffer);
    buffer.set_style(halo, colors.modal_backdrop);
    Clear.render(popup, buffer);
    if overlay == Overlay::Inspector {
        inspector::render(model, theme, popup, buffer, true);
        return;
    }
    let spec = overlay_spec(
        model,
        overlay,
        colors,
        geometry.window,
        geometry.inner.width,
        geometry.inner.height,
    );
    let block = Block::default()
        .title(Line::styled(spec.title, colors.title))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(colors.overlay_border)
        .padding(overlay_padding(overlay));
    let inner = geometry.inner;
    Paragraph::new(spec.content)
        .block(block)
        .style(colors.normal)
        .wrap(Wrap { trim: false })
        .render(popup, buffer);
    if let Some(row) = selection_row(model, overlay, geometry.window) {
        if row < inner.height {
            buffer.set_style(
                Rect::new(inner.x, inner.y + row, inner.width, 1),
                colors.selection_row,
            );
        }
    }
}

fn modal_halo(popup: Rect, area: Rect) -> Rect {
    let x = popup.x.saturating_sub(2).max(area.x);
    let right = popup.right().saturating_add(2).min(area.right());
    Rect::new(
        x,
        popup.y.max(area.y),
        right.saturating_sub(x),
        popup.bottom().min(area.bottom()).saturating_sub(popup.y),
    )
}

fn overlay_spec(
    model: &AppModel,
    overlay: Overlay,
    colors: Palette,
    window: Option<(usize, usize)>,
    content_width: u16,
    content_height: u16,
) -> OverlaySpec {
    match overlay {
        Overlay::CommandPalette => OverlaySpec {
            title: " Command palette ".into(),
            content: palette_text(model, colors, window.unwrap_or((0, 0)), content_width),
        },
        Overlay::Help => OverlaySpec {
            title: " Keyboard guide ".into(),
            content: help_text(colors, content_width),
        },
        Overlay::SessionPicker => OverlaySpec {
            title: " Switch session ".into(),
            content: session_picker_text(model, colors, window.unwrap_or((0, 0))),
        },
        Overlay::TurnNavigator => OverlaySpec {
            title: " Jump to a Turn ".into(),
            content: turn_navigator_text(model, colors, window.unwrap_or((0, 0)), content_width),
        },
        Overlay::PromptHistory => OverlaySpec {
            title: " Prompt history ".into(),
            content: history_text(model, colors, window.unwrap_or((0, 0))),
        },
        Overlay::Inspector => unreachable!("Inspector owns its composite renderer"),
        Overlay::Suspension => {
            decision_sheet_spec(model, overlay, colors, content_width, content_height)
        }
        Overlay::UnknownCommand
        | Overlay::AbandonConfirmation
        | Overlay::ErrorDetails
        | Overlay::EphemeralConfirmation
        | Overlay::QuitConfirmation => {
            decision_sheet_spec(model, overlay, colors, content_width, content_height)
        }
    }
}

fn selection_row(
    model: &AppModel,
    overlay: Overlay,
    window: Option<(usize, usize)>,
) -> Option<u16> {
    let (selection, count) = match overlay {
        Overlay::CommandPalette => (
            model.command_selection,
            model.matching_command_indices().len(),
        ),
        Overlay::SessionPicker => (model.session_selection, model.matching_sessions().count()),
        Overlay::TurnNavigator => (
            model.turn_selection,
            model.matching_landmark_indices().len(),
        ),
        Overlay::PromptHistory => (model.history_selection, model.matching_history().count()),
        Overlay::Inspector => return None,
        _ => return None,
    };
    if selection >= count {
        return None;
    }
    let window_start = window?.0;
    u16::try_from(selection.checked_sub(window_start)?)
        .ok()?
        .checked_add(1)
}

fn turn_navigator_text(
    model: &AppModel,
    colors: Palette,
    (start, end): (usize, usize),
    content_width: u16,
) -> Text<'static> {
    let mut rows = vec![search_line("Search", &model.turn_filter, colors)];
    let matches = model.matching_landmark_indices();
    let ordinal_width = model.conversation_landmarks.len().max(1).to_string().len();
    let preview_width = usize::from(content_width).saturating_sub(ordinal_width + 5);
    rows.extend(
        matches[start..end]
            .iter()
            .enumerate()
            .map(|(offset, index)| {
                let landmark = &model.conversation_landmarks[*index];
                let marker = if start + offset == model.turn_selection {
                    "›"
                } else {
                    " "
                };
                Line::from(vec![
                    Span::styled(format!("{marker} "), colors.selected),
                    Span::styled(
                        format!("{:>ordinal_width$}  ", landmark.ordinal),
                        colors.accent,
                    ),
                    Span::styled(
                        truncate_display(&safe_text(&landmark.prompt_preview), preview_width),
                        colors.normal,
                    ),
                ])
            }),
    );
    if rows.len() == 1 {
        rows.push(Line::styled("  No matching Turns", colors.muted));
    }
    if content_width < 50 {
        rows.push(key_hints(
            &[("↑/↓", "select"), ("Home/End", "edge")],
            colors,
        ));
        rows.push(key_hints(&[("Enter", "jump"), ("Esc", "close")], colors));
    } else {
        rows.push(Line::default());
        rows.push(key_hints(
            &[
                ("↑/↓", "select"),
                ("Home/End", "edge"),
                ("Enter", "jump"),
                ("Esc", "close"),
            ],
            colors,
        ));
    }
    Text::from(rows)
}

fn session_picker_text(
    model: &AppModel,
    colors: Palette,
    (start, end): (usize, usize),
) -> Text<'static> {
    let mut rows = vec![search_line("Filter", &model.session_filter, colors)];
    let matches = model.matching_sessions().collect::<Vec<_>>();
    rows.extend(
        matches[start..end]
            .iter()
            .enumerate()
            .map(|(offset, session)| {
                let ordinal = model
                    .sessions
                    .iter()
                    .position(|item| item.session_id == session.session_id)
                    .map(|index| index + 1)
                    .unwrap_or(start + offset + 1);
                picker_line(
                    session,
                    ordinal,
                    start + offset == model.session_selection,
                    colors,
                )
            })
            .collect::<Vec<_>>(),
    );
    if rows.len() == 1 {
        rows.push(Line::styled("  No matching Sessions", colors.muted));
    }
    rows.push(Line::default());
    rows.push(if model.sessions_loading {
        Line::styled("Loading older Sessions…", colors.muted)
    } else if model.sessions_next_before.is_some() {
        key_hints(
            &[
                ("↑/↓", "select"),
                ("↓", "load more"),
                ("Enter", "open"),
                ("Esc", "close"),
            ],
            colors,
        )
    } else {
        key_hints(
            &[("↑/↓", "select"), ("Enter", "open"), ("Esc", "close")],
            colors,
        )
    });
    Text::from(rows)
}

fn history_text(model: &AppModel, colors: Palette, (start, end): (usize, usize)) -> Text<'static> {
    let mut rows = vec![search_line("Search", &model.history_filter, colors)];
    let matches = model.matching_history().collect::<Vec<_>>();
    rows.extend(
        matches[start..end]
            .iter()
            .enumerate()
            .map(|(offset, text)| {
                let marker = if start + offset == model.history_selection {
                    "›"
                } else {
                    " "
                };
                let first = text.lines().next().unwrap_or_default();
                let preview = first.chars().take(46).collect::<String>();
                Line::from(vec![
                    Span::styled(format!("{marker} "), colors.selected),
                    Span::styled(preview, colors.normal),
                ])
            })
            .collect::<Vec<_>>(),
    );
    if rows.len() == 1 {
        rows.push(Line::styled("  No local prompt history", colors.muted));
    }
    rows.push(Line::default());
    rows.push(key_hints(
        &[("↑/↓", "select"), ("Enter", "restore"), ("Esc", "close")],
        colors,
    ));
    Text::from(rows)
}

fn palette_text(
    model: &AppModel,
    colors: Palette,
    (start, end): (usize, usize),
    content_width: u16,
) -> Text<'static> {
    let mut rows = vec![search_line("Search", &model.command_filter, colors)];
    let matches = model.matching_command_indices();
    rows.extend(
        matches[start..end]
            .iter()
            .enumerate()
            .map(|(offset, index)| {
                let command = COMMAND_PALETTE[*index];
                let marker = if start + offset == model.command_selection {
                    "›"
                } else {
                    " "
                };
                let reason = command
                    .unavailable_reason(model.command_context())
                    .map(|reason| format!("unavailable · {reason}"));
                let detail = truncate_display(
                    reason.as_deref().unwrap_or(command.help),
                    usize::from(content_width).saturating_sub(20),
                );
                Line::from(vec![
                    Span::styled(format!("{marker} "), colors.selected),
                    Span::styled(format!("{:<18}", command.input), colors.accent),
                    Span::styled(
                        detail,
                        if reason.is_some() {
                            colors.warning
                        } else {
                            colors.normal
                        },
                    ),
                ])
            })
            .collect::<Vec<_>>(),
    );
    if rows.len() == 1 {
        rows.push(Line::styled("  No matching commands", colors.muted));
    }
    rows.push(key_hints(
        &[("↑/↓", "select"), ("Enter", "run"), ("Esc", "close")],
        colors,
    ));
    Text::from(rows)
}

fn decision_sheet_spec(
    model: &AppModel,
    overlay: Overlay,
    colors: Palette,
    content_width: u16,
    content_height: u16,
) -> OverlaySpec {
    let spec = decision_sheet::project(model, overlay).expect("decision overlay has a spec");
    let marker = match spec.tone {
        decision_sheet::DecisionSheetTone::Neutral => None,
        decision_sheet::DecisionSheetTone::Warning => Some(("!  ", colors.warning)),
        decision_sheet::DecisionSheetTone::Danger => Some(("×  ", colors.danger)),
    };
    let mut lines = marker.map_or_else(Vec::new, |(marker, style)| {
        vec![Line::styled(marker, style)]
    });
    lines.extend(
        spec.body
            .iter()
            .map(|line| Line::styled(safe_text(line), colors.normal)),
    );
    let mut primary = None;
    if let Some(response) = spec.response {
        let (label, guidance) = match response {
            decision_sheet::DecisionResponseSpec::Editor {
                guidance,
                draft,
                cursor,
            } => {
                let line = Line::styled(
                    editor_view(&draft, cursor, usize::from(content_width.saturating_sub(2))),
                    if draft.is_empty() {
                        colors.placeholder
                    } else {
                        colors.normal
                    },
                );
                primary = Some(line.clone());
                lines.push(line);
                ("Response", guidance)
            }
            decision_sheet::DecisionResponseSpec::Choices {
                guidance,
                choices,
                selected,
            } => {
                lines.extend(choices.into_iter().enumerate().map(|(index, choice)| {
                    let line = Line::styled(
                        format!(
                            "{} {}",
                            if index == selected { "›" } else { " " },
                            truncate_display(
                                &safe_text(&choice),
                                usize::from(content_width.saturating_sub(3)),
                            )
                        ),
                        if index == selected {
                            colors.selected
                        } else {
                            colors.normal
                        },
                    );
                    if index == selected {
                        primary = Some(line.clone());
                    }
                    line
                }));
                ("Choose", guidance)
            }
            decision_sheet::DecisionResponseSpec::ReadOnly { guidance } => ("Read only", guidance),
        };
        lines.extend([
            Line::default(),
            Line::styled(label, colors.title),
            Line::styled(guidance, colors.normal),
        ]);
    }
    let action_lines = decision_sheet::action_groups(&spec.actions, content_width)
        .into_iter()
        .map(|group| {
            let actions = group
                .iter()
                .map(|action| (action.visual_key, action.action))
                .collect::<Vec<_>>();
            key_hints(&actions, colors)
        })
        .collect::<Vec<_>>();
    let capacity = usize::from(content_height);
    if lines.len().saturating_add(action_lines.len()) > capacity {
        let mut compact = lines.into_iter().take(1).collect::<Vec<_>>();
        if let Some(primary) = primary {
            compact.push(primary);
        }
        lines = compact;
    }
    if !action_lines.is_empty() {
        lines.truncate(capacity.saturating_sub(action_lines.len()));
        while lines.len().saturating_add(action_lines.len()) < capacity {
            lines.push(Line::default());
        }
        lines.extend(action_lines);
    }
    OverlaySpec {
        title: format!(" {} ", spec.title),
        content: Text::from(lines),
    }
}

fn editor_view(draft: &str, cursor: usize, width: usize) -> String {
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
        if used.saturating_add(candidate) > budget {
            break;
        }
        used += candidate;
        end += 1;
    }
    let mut visible = graphemes[start..end].to_vec();
    visible.insert(cursor - start, "▏");
    format!("› {}", safe_text(&visible.concat()))
}

fn help_text(colors: Palette, content_width: u16) -> Text<'static> {
    let mut rows = Vec::<Vec<(&str, &str)>>::new();
    for hint in help_hints() {
        let item = (hint.visual_key, hint.action);
        let fits = rows.last().is_some_and(|row| {
            let used = 1 + row
                .iter()
                .enumerate()
                .map(|(index, (key, action))| {
                    usize::from(index != 0) * 2 + key.width() + action.width() + 2
                })
                .sum::<usize>();
            used + usize::from(!row.is_empty()) * 2 + item.0.width() + item.1.width() + 2
                <= usize::from(content_width)
        });
        if fits {
            rows.last_mut()
                .expect("a fitting help row exists")
                .push(item);
        } else {
            rows.push(vec![item]);
        }
    }
    let mut lines = rows
        .iter()
        .map(|row| key_hints(row, colors))
        .collect::<Vec<_>>();
    lines.push(Line::default());
    lines.extend(
        HELP_NOTES
            .iter()
            .map(|note| Line::styled((*note).to_owned(), colors.muted)),
    );
    Text::from(lines)
}

fn search_line(label: &str, value: &str, colors: Palette) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}  "), colors.title),
        Span::styled(
            if value.is_empty() {
                "type to search".into()
            } else {
                safe_text(value)
            },
            if value.is_empty() {
                colors.placeholder
            } else {
                colors.normal
            },
        ),
    ])
}
