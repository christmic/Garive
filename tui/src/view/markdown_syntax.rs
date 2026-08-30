use std::{str::FromStr, sync::OnceLock};

use ratatui::{
    style::{Modifier, Style},
    text::Span,
};
use syntect::{
    easy::HighlightLines,
    highlighting::{
        Color, FontStyle, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings,
    },
    parsing::SyntaxSet,
};

use super::style::Palette;

const MAX_HIGHLIGHT_LINE_BYTES: usize = 16 * 1024;
const MAX_HIGHLIGHT_BLOCK_BYTES: usize = 64 * 1024;

const NORMAL: Color = marker(1);
const COMMENT: Color = marker(2);
const STRING: Color = marker(3);
const CONSTANT: Color = marker(4);
const KEYWORD: Color = marker(5);
const TYPE: Color = marker(6);
const FUNCTION: Color = marker(7);
const PUNCTUATION: Color = marker(8);

#[derive(Clone, Copy)]
pub(super) struct SyntaxPalette {
    normal: Style,
    comment: Style,
    string: Style,
    constant: Style,
    keyword: Style,
    type_name: Style,
    function: Style,
    punctuation: Style,
}

impl SyntaxPalette {
    pub(super) fn from_palette(colors: Palette) -> Self {
        let monochrome = colors.normal.fg == colors.success.fg;
        Self {
            normal: colors.normal,
            comment: colors.muted.add_modifier(Modifier::ITALIC),
            string: if monochrome {
                colors.success.add_modifier(Modifier::ITALIC)
            } else {
                colors.success
            },
            constant: if monochrome {
                colors.warning.add_modifier(Modifier::UNDERLINED)
            } else {
                colors.warning
            },
            keyword: colors.agent.add_modifier(Modifier::BOLD),
            type_name: colors.user,
            function: colors.badge.add_modifier(if monochrome {
                Modifier::UNDERLINED
            } else {
                Modifier::BOLD
            }),
            punctuation: colors.muted,
        }
    }

    fn style(self, marker: Color, font: FontStyle) -> Style {
        let mut style = match marker {
            COMMENT => self.comment,
            STRING => self.string,
            CONSTANT => self.constant,
            KEYWORD => self.keyword,
            TYPE => self.type_name,
            FUNCTION => self.function,
            PUNCTUATION => self.punctuation,
            _ => self.normal,
        };
        if font.contains(FontStyle::BOLD) {
            style = style.add_modifier(Modifier::BOLD);
        }
        if font.contains(FontStyle::ITALIC) {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if font.contains(FontStyle::UNDERLINE) {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        style
    }
}

pub(super) struct CodeHighlighter {
    lines: HighlightLines<'static>,
    palette: SyntaxPalette,
    bytes: usize,
    enabled: bool,
}

impl CodeHighlighter {
    pub(super) fn new(language: &str, palette: SyntaxPalette) -> Option<Self> {
        let syntaxes = syntax_set();
        let syntax = syntaxes.find_syntax_by_token(language)?;
        Some(Self {
            lines: HighlightLines::new(syntax, marker_theme()),
            palette,
            bytes: 0,
            enabled: true,
        })
    }

    pub(super) fn highlight_line(&mut self, line: &str) -> Vec<Span<'static>> {
        self.bytes = self.bytes.saturating_add(line.len());
        if line.len() > MAX_HIGHLIGHT_LINE_BYTES || self.bytes > MAX_HIGHLIGHT_BLOCK_BYTES {
            self.enabled = false;
        }
        if !self.enabled {
            return vec![Span::styled(line.to_owned(), self.palette.normal)];
        }
        match self.lines.highlight_line(line, syntax_set()) {
            Ok(parts) => parts
                .into_iter()
                .map(|(style, value)| {
                    Span::styled(
                        value.to_owned(),
                        self.palette.style(style.foreground, style.font_style),
                    )
                })
                .collect(),
            Err(_) => {
                self.enabled = false;
                vec![Span::styled(line.to_owned(), self.palette.normal)]
            }
        }
    }
}

fn syntax_set() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_nonewlines)
}

fn marker_theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| Theme {
        settings: ThemeSettings {
            foreground: Some(NORMAL),
            ..ThemeSettings::default()
        },
        scopes: vec![
            rule("comment", COMMENT, FontStyle::ITALIC),
            rule("string", STRING, FontStyle::empty()),
            rule(
                "constant.numeric, constant.language, constant.character",
                CONSTANT,
                FontStyle::empty(),
            ),
            rule("keyword, storage", KEYWORD, FontStyle::BOLD),
            rule(
                "entity.name.type, support.type, storage.type",
                TYPE,
                FontStyle::empty(),
            ),
            rule(
                "entity.name.function, support.function",
                FUNCTION,
                FontStyle::empty(),
            ),
            rule("punctuation", PUNCTUATION, FontStyle::empty()),
        ],
        ..Theme::default()
    })
}

fn rule(selector: &str, foreground: Color, font_style: FontStyle) -> ThemeItem {
    ThemeItem {
        scope: ScopeSelectors::from_str(selector).expect("static syntax scope selector"),
        style: StyleModifier {
            foreground: Some(foreground),
            font_style: Some(font_style),
            ..StyleModifier::default()
        },
    }
}

const fn marker(role: u8) -> Color {
    Color {
        r: role,
        g: 0,
        b: 0,
        a: 255,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color as TerminalColor;

    #[test]
    fn known_language_is_semantic_and_unknown_language_is_plain() {
        let palette = test_palette();
        let mut rust = CodeHighlighter::new("rust", palette).expect("bundled Rust grammar");
        let spans = rust.highlight_line("fn answer() -> u64 { 42 }");
        assert!(spans.iter().any(|span| span.content == "fn"));
        assert!(spans.iter().any(|span| span.style.fg != palette.normal.fg));
        assert!(CodeHighlighter::new("not-a-real-language", palette).is_none());
    }

    #[test]
    fn oversized_input_disables_highlighting_for_the_rest_of_the_block() {
        let palette = test_palette();
        let mut rust = CodeHighlighter::new("rust", palette).expect("bundled Rust grammar");
        let long = "x".repeat(MAX_HIGHLIGHT_LINE_BYTES + 1);
        assert_eq!(rust.highlight_line(&long)[0].style.fg, palette.normal.fg);
        assert_eq!(
            rust.highlight_line("fn later() {}")[0].style.fg,
            palette.normal.fg
        );
    }

    fn test_palette() -> SyntaxPalette {
        SyntaxPalette {
            normal: Style::default().fg(TerminalColor::White),
            comment: Style::default().fg(TerminalColor::DarkGray),
            string: Style::default().fg(TerminalColor::Green),
            constant: Style::default().fg(TerminalColor::Yellow),
            keyword: Style::default().fg(TerminalColor::Blue),
            type_name: Style::default().fg(TerminalColor::Magenta),
            function: Style::default().fg(TerminalColor::Cyan),
            punctuation: Style::default().fg(TerminalColor::Gray),
        }
    }
}
