use ratatui::{
    style::{Color, Modifier, Style},
    symbols,
};

use crate::{application::ConnectionState, Theme};

use super::terminal_profile::{self, ColorLevel, TerminalProfile};

const ASCII_BORDER: symbols::border::Set<'static> = symbols::border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

#[derive(Clone, Copy)]
pub(super) struct Palette {
    unicode_borders: bool,
    pub(super) normal: Style,
    pub(super) muted: Style,
    pub(super) accent: Style,
    pub(super) title: Style,
    pub(super) badge: Style,
    pub(super) border: Style,
    pub(super) overlay_border: Style,
    pub(super) modal_backdrop: Style,
    pub(super) selected: Style,
    pub(super) user: Style,
    pub(super) request_surface: Style,
    pub(super) request_marker: Style,
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

impl Palette {
    pub(super) const fn border_set(self) -> symbols::border::Set<'static> {
        if self.unicode_borders {
            symbols::border::ROUNDED
        } else {
            ASCII_BORDER
        }
    }
}

pub(super) fn palette(theme: Theme) -> Palette {
    palette_for(theme, terminal_profile::current())
}

pub(super) fn palette_for(theme: Theme, profile: TerminalProfile) -> Palette {
    let color_level = profile.effective_color(theme);
    let mono = color_level == ColorLevel::Mono;
    let (accent, violet, surface, text, muted) = if theme == Theme::Light {
        (
            Color::Blue,
            Color::Magenta,
            adaptive((235, 238, 244), Color::White, color_level),
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
            adaptive((72, 202, 228), Color::Cyan, color_level),
            adaptive((189, 147, 249), Color::Magenta, color_level),
            adaptive((24, 28, 38), Color::Black, color_level),
            adaptive((232, 235, 242), Color::White, color_level),
            adaptive((126, 134, 151), Color::DarkGray, color_level),
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
                adaptive((214, 229, 255), Color::White, color_level)
            } else {
                adaptive((40, 55, 68), Color::Black, color_level)
            })
            .add_modifier(Modifier::BOLD)
    };
    let text_selection = if mono {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(text).bg(violet)
    };
    Palette {
        unicode_borders: profile.unicode_borders(),
        normal: Style::default().fg(text),
        muted: Style::default().fg(muted),
        accent: bold,
        title: strong,
        badge: Style::default().fg(violet),
        border: Style::default().fg(muted),
        overlay_border: Style::default().fg(violet),
        modal_backdrop: Style::default().add_modifier(Modifier::DIM),
        selected: Style::default().fg(accent).add_modifier(Modifier::BOLD),
        user: Style::default().fg(violet).add_modifier(Modifier::BOLD),
        request_surface: if mono {
            Style::default().fg(text)
        } else {
            Style::default().fg(text).bg(surface)
        },
        request_marker: if mono {
            Style::default().fg(text).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(accent)
                .bg(surface)
                .add_modifier(Modifier::BOLD)
        },
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

fn adaptive(target: (u8, u8, u8), basic: Color, level: ColorLevel) -> Color {
    match level {
        ColorLevel::TrueColor => Color::Rgb(target.0, target.1, target.2),
        ColorLevel::Ansi256 => Color::Indexed(nearest_xterm_index(target)),
        ColorLevel::Basic => basic,
        ColorLevel::Mono => Color::Reset,
    }
}

fn nearest_xterm_index(target: (u8, u8, u8)) -> u8 {
    let levels = [0_u8, 95, 135, 175, 215, 255];
    let mut best = (u32::MAX, 16_u8);
    for red in 0..6 {
        for green in 0..6 {
            for blue in 0..6 {
                let index = 16 + 36 * red + 6 * green + blue;
                let candidate = (levels[red], levels[green], levels[blue]);
                best = nearer(target, candidate, index as u8, best);
            }
        }
    }
    for step in 0..24 {
        let value = 8 + step * 10;
        best = nearer(target, (value, value, value), 232 + step, best);
    }
    best.1
}

fn nearer(target: (u8, u8, u8), candidate: (u8, u8, u8), index: u8, best: (u32, u8)) -> (u32, u8) {
    let channel = |left: u8, right: u8| u32::from(left.abs_diff(right)).pow(2);
    let distance = channel(target.0, candidate.0)
        + channel(target.1, candidate.1)
        + channel(target.2, candidate.2);
    if distance < best.0 {
        (distance, index)
    } else {
        best
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

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(term: &str, colorterm: Option<&str>, locale: &str) -> TerminalProfile {
        TerminalProfile::detect(Some(term), colorterm, Some(locale))
    }

    #[test]
    fn rgb_tokens_downgrade_to_the_admitted_color_vocabulary() {
        let truecolor = palette_for(
            Theme::Dark,
            profile("xterm-256color", Some("truecolor"), "en_US.UTF-8"),
        );
        assert_eq!(truecolor.accent.fg, Some(Color::Rgb(72, 202, 228)));

        let ansi256 = palette_for(Theme::Dark, profile("xterm-256color", None, "en_US.UTF-8"));
        assert!(matches!(ansi256.accent.fg, Some(Color::Indexed(_))));
        assert!(matches!(
            ansi256.request_surface.bg,
            Some(Color::Indexed(_))
        ));

        let basic = palette_for(Theme::Dark, profile("xterm", None, "C"));
        assert_eq!(basic.accent.fg, Some(Color::Cyan));
        assert_eq!(basic.request_surface.bg, Some(Color::Black));

        let mono = palette_for(Theme::Mono, TerminalProfile::default());
        assert_eq!(mono.request_surface.bg, None);
        assert_eq!(mono.accent.fg, Some(Color::Reset));
    }

    #[test]
    fn border_symbols_follow_the_utf8_capability() {
        let unicode = palette_for(Theme::Dark, profile("xterm", None, "C.UTF-8"));
        assert_eq!(unicode.border_set(), symbols::border::ROUNDED);

        let ascii = palette_for(Theme::Dark, profile("xterm", None, "C"));
        assert_eq!(ascii.border_set(), ASCII_BORDER);
    }
}
