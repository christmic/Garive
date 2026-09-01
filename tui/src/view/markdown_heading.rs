//! Heading marker and semantic emphasis shared by Markdown presentations.

use pulldown_cmark::HeadingLevel;
use ratatui::style::{Modifier, Style};

pub(super) fn heading_marker(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "# ",
        HeadingLevel::H2 => "## ",
        HeadingLevel::H3 => "### ",
        HeadingLevel::H4 => "#### ",
        HeadingLevel::H5 => "##### ",
        HeadingLevel::H6 => "###### ",
    }
}

pub(super) fn heading_style(level: HeadingLevel, accent: Style) -> Style {
    match level {
        HeadingLevel::H1 => accent.add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        HeadingLevel::H2 => accent.add_modifier(Modifier::BOLD),
        _ => accent.add_modifier(Modifier::ITALIC),
    }
}
