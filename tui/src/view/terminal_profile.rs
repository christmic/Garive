//! Immutable terminal rendering capabilities captured before the first frame.

use std::sync::OnceLock;

use crate::Theme;

/// Effective color vocabulary available to one render.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ColorLevel {
    /// Text attributes only; selected by the monochrome theme.
    Mono,
    /// The portable named ANSI color set.
    Basic,
    /// The fixed 256-color xterm palette.
    Ansi256,
    /// Direct 24-bit RGB colors.
    TrueColor,
}

/// Process-local terminal facts that cannot change during one TUI run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalProfile {
    color: ColorLevel,
    unicode_borders: bool,
}

impl TerminalProfile {
    /// Resolves only capabilities explicitly advertised by environment values.
    pub(crate) fn detect(
        term: Option<&str>,
        colorterm: Option<&str>,
        locale: Option<&str>,
    ) -> Self {
        let term = term.unwrap_or_default().to_ascii_lowercase();
        let colorterm = colorterm.unwrap_or_default().to_ascii_lowercase();
        let color = if term == "dumb" {
            ColorLevel::Mono
        } else if matches!(colorterm.as_str(), "truecolor" | "24bit") {
            ColorLevel::TrueColor
        } else if term.contains("256color") {
            ColorLevel::Ansi256
        } else {
            ColorLevel::Basic
        };
        let locale = locale.unwrap_or_default().to_ascii_lowercase();
        let unicode_borders =
            color != ColorLevel::Mono && (locale.contains("utf-8") || locale.contains("utf8"));
        Self {
            color,
            unicode_borders,
        }
    }

    pub(crate) const fn effective_color(self, theme: Theme) -> ColorLevel {
        if matches!(theme, Theme::Mono) {
            ColorLevel::Mono
        } else {
            self.color
        }
    }

    pub(crate) const fn unicode_borders(self) -> bool {
        self.unicode_borders
    }
}

impl Default for TerminalProfile {
    fn default() -> Self {
        Self {
            color: ColorLevel::TrueColor,
            unicode_borders: true,
        }
    }
}

static STARTUP_PROFILE: OnceLock<TerminalProfile> = OnceLock::new();

pub(crate) fn install(profile: TerminalProfile) {
    let _ = STARTUP_PROFILE.set(profile);
}

pub(crate) fn current() -> TerminalProfile {
    STARTUP_PROFILE.get().copied().unwrap_or_default()
}

pub(crate) fn detect_process() -> TerminalProfile {
    let term = std::env::var("TERM").ok();
    let colorterm = std::env::var("COLORTERM").ok();
    let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()));
    TerminalProfile::detect(term.as_deref(), colorterm.as_deref(), locale.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_admits_only_explicit_color_claims() {
        assert_eq!(
            TerminalProfile::detect(Some("xterm-256color"), None, Some("en_US.UTF-8")).color,
            ColorLevel::Ansi256
        );
        assert_eq!(
            TerminalProfile::detect(
                Some("xterm-256color"),
                Some("truecolor"),
                Some("en_US.UTF-8"),
            )
            .color,
            ColorLevel::TrueColor
        );
        assert_eq!(
            TerminalProfile::detect(Some("xterm"), None, Some("C")).color,
            ColorLevel::Basic
        );
        assert_eq!(
            TerminalProfile::detect(Some("dumb"), Some("truecolor"), Some("en_US.UTF-8")).color,
            ColorLevel::Mono
        );
    }

    #[test]
    fn utf8_locale_is_the_only_unicode_border_admission() {
        assert!(TerminalProfile::detect(Some("xterm"), None, Some("C.UTF-8")).unicode_borders());
        assert!(!TerminalProfile::detect(Some("xterm"), None, Some("C")).unicode_borders());
        assert!(!TerminalProfile::detect(Some("dumb"), None, Some("C.UTF-8")).unicode_borders());
    }

    #[test]
    fn monochrome_theme_is_authoritative_over_color_capability() {
        let profile = TerminalProfile::default();
        assert_eq!(profile.effective_color(Theme::Mono), ColorLevel::Mono);
        assert_eq!(profile.effective_color(Theme::Dark), ColorLevel::TrueColor);
    }
}
