use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Paragraph, Widget, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    application::{AppModel, Overlay},
    input::help_hints,
    Theme,
};

use super::{
    decision_sheet, inspector, palette,
    presentation::HELP_NOTES,
    primitives::{key_hints, truncate_display, BottomPaneFrame, ModalFrame, SelectionRow},
    safe_text,
    session::picker_line,
    style::Palette,
};

pub(super) mod command_palette;
pub(super) mod filtered_list;
pub(super) mod geometry;

use geometry::{overlay_geometry, overlay_padding};

struct OverlaySpec {
    title: String,
    content: Text<'static>,
}

pub(super) const fn is_composition_selector(overlay: Overlay) -> bool {
    matches!(
        overlay,
        Overlay::SessionPicker | Overlay::TurnNavigator | Overlay::PromptHistory
    )
}

pub(super) const fn uses_bottom_pane(overlay: Overlay) -> bool {
    is_composition_selector(overlay) || matches!(overlay, Overlay::Inspector)
}

pub(super) fn render_overlay(
    model: &AppModel,
    overlay: Overlay,
    theme: Theme,
    area: Rect,
    buffer: &mut Buffer,
) {
    let colors = palette(theme);
    if overlay == Overlay::CommandPalette {
        command_palette::render(model, colors, area, buffer);
        return;
    }
    let geometry = overlay_geometry(model, overlay, area);
    let popup = geometry.popup;
    if overlay == Overlay::Inspector {
        BottomPaneFrame::resolve(popup).render(
            Line::styled(inspector::title(model), colors.title),
            colors,
            buffer,
        );
        inspector::render_inner(model, theme, geometry.inner, buffer);
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
    let inner = geometry.inner;
    if is_composition_selector(overlay) {
        BottomPaneFrame::resolve(popup).render(
            Line::styled(spec.title, colors.title),
            colors,
            buffer,
        );
    } else {
        ModalFrame::resolve(popup, overlay_padding(overlay)).render(
            area,
            Line::styled(spec.title, colors.title),
            colors,
            buffer,
        );
    }
    let paragraph = Paragraph::new(spec.content).style(colors.normal);
    if matches!(overlay, Overlay::SessionPicker | Overlay::PromptHistory) {
        paragraph.render(inner, buffer);
    } else {
        paragraph.wrap(Wrap { trim: false }).render(inner, buffer);
    }
    if let Some(row) = selection_row(model, overlay, geometry.window) {
        if row < inner.height {
            SelectionRow::full_area(true).paint(
                Rect::new(inner.x, inner.y + row, inner.width, 1),
                colors,
                buffer,
            );
        }
    }
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
        Overlay::CommandPalette => unreachable!("CommandPalette owns its composite renderer"),
        Overlay::Help => OverlaySpec {
            title: " Keyboard guide ".into(),
            content: help_text(colors, content_width),
        },
        Overlay::SessionPicker => OverlaySpec {
            title: " Switch session ".into(),
            content: session_picker_text(
                model,
                colors,
                window.unwrap_or((0, 0)),
                content_width,
                content_height,
            ),
        },
        Overlay::TurnNavigator => OverlaySpec {
            title: " Jump to a Turn ".into(),
            content: turn_navigator_text(model, colors, window.unwrap_or((0, 0)), content_width),
        },
        Overlay::PromptHistory => OverlaySpec {
            title: " Prompt history ".into(),
            content: history_text(
                model,
                colors,
                window.unwrap_or((0, 0)),
                content_width,
                content_height,
            ),
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
        Overlay::CommandPalette => unreachable!("CommandPalette owns its selection row"),
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
                let selected = start + offset == model.turn_selection;
                Line::from(vec![
                    SelectionRow::full_area(selected).marker(colors),
                    Span::styled(
                        format!("{:>ordinal_width$}  ", landmark.ordinal),
                        if selected {
                            colors.selected
                        } else {
                            colors.muted
                        },
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
    content_width: u16,
    content_height: u16,
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
                let line = picker_line(
                    session,
                    ordinal,
                    start + offset == model.session_selection,
                    colors,
                );
                truncate_line(line, usize::from(content_width))
            })
            .collect::<Vec<_>>(),
    );
    if rows.len() == 1 {
        rows.push(Line::styled("  No matching Sessions", colors.muted));
    }
    pad_for_action(&mut rows, content_height);
    rows.push(if content_width < 50 {
        key_hints(&[("Enter", "open"), ("Esc", "close")], colors)
    } else if model.sessions_loading {
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

fn history_text(
    model: &AppModel,
    colors: Palette,
    (start, end): (usize, usize),
    content_width: u16,
    content_height: u16,
) -> Text<'static> {
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
                let preview = truncate_display(
                    &safe_text(first),
                    usize::from(content_width).saturating_sub(2),
                );
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
    pad_for_action(&mut rows, content_height);
    rows.push(if content_width < 50 {
        key_hints(&[("Enter", "restore"), ("Esc", "close")], colors)
    } else {
        key_hints(
            &[("↑/↓", "select"), ("Enter", "restore"), ("Esc", "close")],
            colors,
        )
    });
    Text::from(rows)
}

fn pad_for_action(rows: &mut Vec<Line<'static>>, content_height: u16) {
    let action_index = usize::from(content_height.saturating_sub(1));
    rows.resize_with(action_index.max(rows.len()), Line::default);
}

fn truncate_line(line: Line<'static>, width: usize) -> Line<'static> {
    let mut remaining = width;
    let mut spans = Vec::new();
    for span in line.spans {
        if remaining == 0 {
            break;
        }
        let content = truncate_display(span.content.as_ref(), remaining);
        remaining = remaining.saturating_sub(content.width());
        spans.push(Span::styled(content, span.style));
    }
    Line::from(spans)
}

fn decision_sheet_spec(
    model: &AppModel,
    overlay: Overlay,
    colors: Palette,
    content_width: u16,
    content_height: u16,
) -> OverlaySpec {
    let spec = decision_sheet::project(model, overlay).expect("decision overlay has a spec");
    let lines: Vec<Line<'static>> =
        decision_sheet::layout(&spec, content_width, usize::from(content_height))
            .rows
            .into_iter()
            .map(|row| match row {
                decision_sheet::DecisionRow::Body { value, tone } => match tone {
                    Some(decision_sheet::DecisionSheetTone::Warning) => Line::from(vec![
                        Span::styled("!  ", colors.warning),
                        Span::styled(value, colors.normal),
                    ]),
                    Some(decision_sheet::DecisionSheetTone::Danger) => Line::from(vec![
                        Span::styled("×  ", colors.danger),
                        Span::styled(value, colors.normal),
                    ]),
                    _ => Line::styled(value, colors.normal),
                },
                decision_sheet::DecisionRow::Editor {
                    before,
                    after,
                    empty,
                } => {
                    let content = if empty {
                        colors.placeholder
                    } else {
                        colors.normal
                    };
                    Line::from(vec![
                        Span::styled(before, content),
                        Span::styled("▏", colors.accent),
                        Span::styled(after, content),
                    ])
                }
                decision_sheet::DecisionRow::Choice {
                    value,
                    selected,
                    compact_position,
                    ..
                } => {
                    let suffix = compact_position
                        .map(decision_sheet::compact_choice_suffix)
                        .unwrap_or_default();
                    let mut content =
                        format!("{} {value}{suffix}", if selected { "›" } else { " " });
                    if selected {
                        content.extend(std::iter::repeat_n(
                            ' ',
                            usize::from(content_width).saturating_sub(content.width()),
                        ));
                    }
                    Line::styled(
                        content,
                        SelectionRow::full_area(selected).style(colors, colors.normal),
                    )
                }
                decision_sheet::DecisionRow::Blank => Line::default(),
                decision_sheet::DecisionRow::Label(value) => Line::styled(value, colors.title),
                decision_sheet::DecisionRow::Guidance(value) => Line::styled(value, colors.normal),
                decision_sheet::DecisionRow::Actions(group) => key_hints(
                    &group
                        .iter()
                        .map(|action| (action.visual_key, action.action))
                        .collect::<Vec<_>>(),
                    colors,
                ),
            })
            .collect();
    OverlaySpec {
        title: format!(" {} ", spec.title),
        content: Text::from(lines),
    }
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
