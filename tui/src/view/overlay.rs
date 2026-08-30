use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Widget, Wrap},
};

use crate::{
    application::{AppModel, Overlay},
    input::COMMAND_PALETTE,
    Theme,
};

use super::{
    palette, presentation::suspension_copy, primitives::key_hints, safe_text, session::picker_line,
    style::Palette,
};

pub(super) mod geometry;

use geometry::overlay_geometry;

struct OverlaySpec {
    title: &'static str,
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
    let spec = overlay_spec(model, overlay, colors, geometry.window);
    let popup = geometry.popup;
    buffer.set_style(area, colors.modal_backdrop);
    Clear.render(popup, buffer);
    let block = Block::default()
        .title(Line::styled(spec.title, colors.title))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(colors.overlay_border)
        .padding(Padding::new(2, 2, 1, 1));
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

fn overlay_spec(
    model: &AppModel,
    overlay: Overlay,
    colors: Palette,
    window: Option<(usize, usize)>,
) -> OverlaySpec {
    match overlay {
        Overlay::CommandPalette => OverlaySpec {
            title: " Command palette ",
            content: palette_text(model, colors, window.unwrap_or((0, 0))),
        },
        Overlay::Help => OverlaySpec {
            title: " Keyboard guide ",
            content: help_text(colors),
        },
        Overlay::SessionPicker => OverlaySpec {
            title: " Switch session ",
            content: session_picker_text(model, colors, window.unwrap_or((0, 0))),
        },
        Overlay::PromptHistory => OverlaySpec {
            title: " Prompt history ",
            content: history_text(model, colors, window.unwrap_or((0, 0))),
        },
        Overlay::Suspension => OverlaySpec {
            title: " Action required ",
            content: suspension_text(model, colors),
        },
        Overlay::UnknownCommand => OverlaySpec {
            title: " Unknown command ",
            content: action_text(
                model
                    .notice
                    .as_deref()
                    .unwrap_or("Nothing was sent to the Host."),
                &[("Enter", "exact retry"), ("A", "abandon local record")],
                colors,
            ),
        },
        Overlay::ErrorDetails => OverlaySpec {
            title: " Status details ",
            content: action_text(
                model
                    .notice
                    .as_deref()
                    .unwrap_or("No additional safe details."),
                &[("Esc", "close")],
                colors,
            ),
        },
        Overlay::EphemeralConfirmation => OverlaySpec {
            title: " Ephemeral mode ",
            content: action_text(
                "A lost response cannot be recovered after exit.",
                &[("Enter", "accept for this run"), ("Esc", "cancel")],
                colors,
            ),
        },
        Overlay::QuitConfirmation => OverlaySpec {
            title: " Quit Garive? ",
            content: action_text(
                "Your Sessions stay durable in the Host.",
                &[("Enter", "quit"), ("Esc", "keep working")],
                colors,
            ),
        },
    }
}

fn selection_row(
    model: &AppModel,
    overlay: Overlay,
    window: Option<(usize, usize)>,
) -> Option<u16> {
    let selection = match overlay {
        Overlay::CommandPalette => model.command_selection,
        Overlay::SessionPicker => model.session_selection,
        Overlay::PromptHistory => model.history_selection,
        _ => return None,
    };
    let window_start = window?.0;
    u16::try_from(selection.checked_sub(window_start)?)
        .ok()?
        .checked_add(1)
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
                picker_line(session, start + offset == model.session_selection, colors)
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

fn palette_text(model: &AppModel, colors: Palette, (start, end): (usize, usize)) -> Text<'static> {
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
                let disabled = command
                    .unavailable_reason(model.command_context())
                    .map_or_else(String::new, |reason| format!("  · {reason}"));
                Line::from(vec![
                    Span::styled(format!("{marker} "), colors.selected),
                    Span::styled(format!("{:<12} ", command.input), colors.accent),
                    Span::styled(command.help.to_owned(), colors.normal),
                    Span::styled(disabled, colors.muted),
                ])
            })
            .collect::<Vec<_>>(),
    );
    if rows.len() == 1 {
        rows.push(Line::styled("  No matching commands", colors.muted));
    }
    rows.push(Line::default());
    rows.push(key_hints(
        &[("↑/↓", "select"), ("Enter", "run"), ("Esc", "close")],
        colors,
    ));
    Text::from(rows)
}

fn suspension_text(model: &AppModel, colors: Palette) -> Text<'static> {
    let copy = suspension_copy(model.suspension.as_ref());
    let mut lines = vec![
        Line::from(vec![
            Span::styled("!  ", colors.warning),
            Span::styled(copy.title, colors.title),
        ]),
        Line::styled(copy.context, colors.normal),
    ];
    if let Some(message) = copy.message {
        lines.push(Line::styled(safe_text(&message), colors.normal));
    }
    lines.extend([
        Line::default(),
        Line::styled("Response", colors.title),
        Line::styled(copy.guidance, colors.normal),
        Line::default(),
        key_hints(
            &[("Enter", "respond now"), ("Ctrl+Q", "leave safely")],
            colors,
        ),
    ]);
    Text::from(lines)
}

fn help_text(colors: Palette) -> Text<'static> {
    Text::from(vec![
        key_hints(&[("Enter", "send"), ("Ctrl+J", "new line")], colors),
        key_hints(&[("Ctrl+N", "new Session"), ("Ctrl+S", "Sessions")], colors),
        key_hints(
            &[("Ctrl+P", "commands"), ("Ctrl+R", "prompt history")],
            colors,
        ),
        key_hints(&[("Esc", "cancel Turn"), ("Ctrl+Q", "quit")], colors),
        Line::default(),
        Line::styled(
            "Durable truth comes from the local Garive Host.",
            colors.muted,
        ),
    ])
}

fn action_text(body: &str, actions: &[(&str, &str)], colors: Palette) -> Text<'static> {
    Text::from(vec![
        Line::styled(safe_text(body), colors.normal),
        Line::default(),
        key_hints(actions, colors),
    ])
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
