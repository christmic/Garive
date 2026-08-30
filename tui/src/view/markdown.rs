use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::safe_text;

pub(super) fn render_markdown(
    source: &str,
    prefix: &str,
    normal: Style,
    accent: Style,
    muted: Style,
) -> Vec<Line<'static>> {
    let mut renderer = Renderer::new(prefix, normal, accent, muted);
    let options =
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    for event in Parser::new_ext(source, options) {
        renderer.event(event);
    }
    renderer.finish()
}

struct Renderer<'a> {
    prefix: &'a str,
    normal: Style,
    accent: Style,
    muted: Style,
    style: Style,
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    list_depth: usize,
    quote_depth: usize,
    code_block: bool,
}

impl<'a> Renderer<'a> {
    fn new(prefix: &'a str, normal: Style, accent: Style, muted: Style) -> Self {
        Self {
            prefix,
            normal,
            accent,
            muted,
            style: normal,
            lines: Vec::new(),
            spans: Vec::new(),
            list_depth: 0,
            quote_depth: 0,
            code_block: false,
        }
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(value) | Event::Html(value) | Event::InlineHtml(value) => {
                self.text(&safe_text(&value))
            }
            Event::Code(value) => self.push(&safe_text(&value), self.accent),
            Event::SoftBreak => self.push(" ", self.style),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.flush();
                self.push("────────", self.muted);
                self.flush();
            }
            Event::TaskListMarker(checked) => {
                self.push(if checked { "[x] " } else { "[ ] " }, self.muted)
            }
            Event::InlineMath(value) | Event::DisplayMath(value) => {
                self.push(&safe_text(&value), self.accent)
            }
            Event::FootnoteReference(value) => {
                self.push(&format!("[{}]", safe_text(&value)), self.muted)
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush();
                self.style = self.accent.add_modifier(Modifier::BOLD);
                self.push(heading_marker(level), self.style);
            }
            Tag::Paragraph => self.flush(),
            Tag::Strong => self.style = self.style.add_modifier(Modifier::BOLD),
            Tag::Emphasis => self.style = self.style.add_modifier(Modifier::ITALIC),
            Tag::Strikethrough => self.style = self.style.add_modifier(Modifier::CROSSED_OUT),
            Tag::BlockQuote(_) => {
                self.flush();
                self.quote_depth += 1;
            }
            Tag::List(_) => {
                self.flush();
                self.list_depth += 1;
            }
            Tag::Item => {
                self.flush();
                self.push(
                    &format!("{}• ", "  ".repeat(self.list_depth.saturating_sub(1))),
                    self.accent,
                );
            }
            Tag::CodeBlock(_) => {
                self.flush();
                self.code_block = true;
                self.style = self.muted;
            }
            Tag::Table(_) => self.flush(),
            Tag::TableHead | Tag::TableRow => self.flush(),
            Tag::TableCell => {
                if !self.spans.is_empty() {
                    self.push(" │ ", self.muted);
                }
            }
            Tag::Link { .. }
            | Tag::Image { .. }
            | Tag::MetadataBlock(_)
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::HtmlBlock
            | Tag::Superscript
            | Tag::Subscript => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::Item
            | TagEnd::TableRow
            | TagEnd::TableHead => self.flush(),
            TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough => self.style = self.normal,
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            TagEnd::List(_) => {
                self.flush();
                self.list_depth = self.list_depth.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                self.flush();
                self.code_block = false;
                self.style = self.normal;
            }
            TagEnd::Table => self.flush(),
            _ => {}
        }
    }

    fn text(&mut self, value: &str) {
        for (index, line) in value.split('\n').enumerate() {
            if index > 0 {
                self.flush();
            }
            self.push(line, self.style);
        }
    }

    fn push(&mut self, value: &str, style: Style) {
        if !value.is_empty() {
            self.spans.push(Span::styled(value.to_owned(), style));
        }
    }

    fn flush(&mut self) {
        if self.spans.is_empty() {
            return;
        }
        let mut spans = vec![Span::raw(self.prefix.to_owned())];
        if self.quote_depth > 0 {
            spans.push(Span::styled("│ ".repeat(self.quote_depth), self.muted));
        }
        if self.code_block {
            spans.push(Span::styled("▏ ", self.muted));
        }
        spans.append(&mut self.spans);
        self.lines.push(Line::from(spans));
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush();
        self.lines
    }
}

fn heading_marker(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "# ",
        HeadingLevel::H2 => "## ",
        HeadingLevel::H3 => "### ",
        HeadingLevel::H4 => "#### ",
        HeadingLevel::H5 => "##### ",
        HeadingLevel::H6 => "###### ",
    }
}
