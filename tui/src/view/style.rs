use ratatui::style::{Color, Modifier, Style};

use crate::{application::ConnectionState, Theme};

#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub(super) normal: Style,
    pub(super) muted: Style,
    pub(super) accent: Style,
    pub(super) title: Style,
    pub(super) badge: Style,
    pub(super) border: Style,
    pub(super) composer_border: Style,
    pub(super) overlay_border: Style,
    pub(super) modal_backdrop: Style,
    pub(super) selected: Style,
    pub(super) user: Style,
    pub(super) agent: Style,
    pub(super) activity: Style,
    pub(super) placeholder: Style,
    pub(super) empty_title: Style,
    pub(super) keycap: Style,
    pub(super) notice: Style,
    pub(super) success: Style,
    pub(super) warning: Style,
    pub(super) danger: Style,
    pub(super) selection_row: Style,
    pub(super) text_selection: Style,
}

pub(super) fn palette(theme: Theme) -> Palette {
    let mono = theme == Theme::Mono;
    let (accent, violet, surface, text, muted) = if theme == Theme::Light {
        (
            Color::Blue,
            Color::Magenta,
            Color::Rgb(235, 238, 244),
            Color::Black,
            Color::DarkGray,
        )
    } else if mono {
        (
            Color::Reset,
            Color::Reset,
            Color::Reset,
            Color::Reset,
            Color::DarkGray,
        )
    } else {
        (
            Color::Rgb(72, 202, 228),
            Color::Rgb(189, 147, 249),
            Color::Rgb(24, 28, 38),
            Color::Rgb(232, 235, 242),
            Color::Rgb(126, 134, 151),
        )
    };
    let bold = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let strong = Style::default().fg(text).add_modifier(Modifier::BOLD);
    let keycap = if mono {
        Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default()
            .fg(text)
            .bg(surface)
            .add_modifier(Modifier::BOLD)
    };
    let selection_row = if mono {
        Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default()
            .fg(text)
            .bg(if theme == Theme::Light {
                Color::Rgb(214, 229, 255)
            } else {
                Color::Rgb(40, 55, 68)
            })
            .add_modifier(Modifier::BOLD)
    };
    let text_selection = if mono {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(text).bg(violet)
    };
    Palette {
        normal: Style::default().fg(text),
        muted: Style::default().fg(muted),
        accent: bold,
        title: strong,
        badge: Style::default().fg(violet),
        border: Style::default().fg(muted),
        composer_border: Style::default().fg(accent),
        overlay_border: Style::default().fg(violet),
        modal_backdrop: Style::default().add_modifier(Modifier::DIM),
        selected: Style::default().fg(accent).add_modifier(Modifier::BOLD),
        user: Style::default().fg(violet).add_modifier(Modifier::BOLD),
        agent: strong,
        activity: Style::default().fg(if mono { text } else { Color::Yellow }),
        placeholder: Style::default().fg(muted).add_modifier(Modifier::ITALIC),
        empty_title: Style::default().fg(text).add_modifier(Modifier::BOLD),
        keycap,
        notice: Style::default().fg(violet).add_modifier(Modifier::BOLD),
        success: Style::default().fg(if mono { text } else { Color::Green }),
        warning: Style::default().fg(if mono { text } else { Color::Yellow }),
        danger: Style::default().fg(if mono { text } else { Color::Red }),
        selection_row,
        text_selection,
    }
}

pub(super) fn connection_name(value: ConnectionState) -> &'static str {
    match value {
        ConnectionState::Connecting => "connecting",
        ConnectionState::Online => "online",
        ConnectionState::Disconnected { .. } => "disconnected",
        ConnectionState::Reconnecting { .. } => "reconnecting",
        ConnectionState::Unavailable { .. } => "unavailable",
    }
}

pub(super) fn session_state_icon(value: &str) -> &'static str {
    match value {
        "completed" => "✓",
        "running" => "●",
        "suspended" => "!",
        "failed" => "×",
        "stopped" | "cancelled" => "■",
        _ => "○",
    }
}

pub(super) fn session_state_style(value: &str, colors: Palette) -> Style {
    match value {
        "completed" => colors.success,
        "running" => colors.accent,
        "suspended" => colors.warning,
        "failed" => colors.danger,
        _ => colors.muted,
    }
}
