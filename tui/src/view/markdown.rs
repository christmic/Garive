use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::markdown_syntax::{CodeHighlighter, SyntaxPalette};
use super::markdown_table::TableBuilder;
use super::markdown_wrap::wrap_styled;
use super::safe_text;

pub(super) fn render_markdown(
    source: &str,
    prefix: &str,
    normal: Style,
    accent: Style,
    muted: Style,
    syntax: SyntaxPalette,
    width: u16,
) -> Vec<Line<'static>> {
    let mut renderer = Renderer::new(prefix, normal, accent, muted, syntax, width);
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
    syntax: SyntaxPalette,
    block_style: Style,
    inline_styles: Vec<Style>,
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    lists: Vec<ListState>,
    hanging_indents: Vec<usize>,
    quote_depth: usize,
    code_block: bool,
    code_highlighter: Option<CodeHighlighter>,
    link: Option<LinkState>,
    table: Option<TableBuilder>,
    width: usize,
}

enum ListState {
    Unordered,
    Ordered(u64),
}

struct LinkState {
    destination: String,
    label: String,
}

impl<'a> Renderer<'a> {
    fn new(
        prefix: &'a str,
        normal: Style,
        accent: Style,
        muted: Style,
        syntax: SyntaxPalette,
        width: u16,
    ) -> Self {
        Self {
            prefix,
            normal,
            accent,
            muted,
            syntax,
            block_style: normal,
            inline_styles: Vec::new(),
            lines: Vec::new(),
            spans: Vec::new(),
            lists: Vec::new(),
            hanging_indents: Vec::new(),
            quote_depth: 0,
            code_block: false,
            code_highlighter: None,
            link: None,
            table: None,
            width: usize::from(width.max(1)),
        }
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(value) | Event::Html(value) | Event::InlineHtml(value) => {
                self.text(&safe_text(&value))
            }
            Event::Code(value) => {
                let value = safe_text(&value);
                self.record_link_label(&value);
                self.push(&value, self.accent);
            }
            Event::SoftBreak => self.push(" ", self.current_style()),
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
                self.start_block();
                self.block_style = heading_style(level, self.accent);
                self.push(heading_marker(level), self.current_style());
            }
            Tag::Paragraph => self.start_block(),
            Tag::Strong => self.push_inline(Modifier::BOLD),
            Tag::Emphasis => self.push_inline(Modifier::ITALIC),
            Tag::Strikethrough => self.push_inline(Modifier::CROSSED_OUT),
            Tag::BlockQuote(_) => {
                self.start_block();
                self.quote_depth += 1;
            }
            Tag::List(start) => {
                self.start_block();
                self.lists
                    .push(start.map_or(ListState::Unordered, ListState::Ordered));
            }
            Tag::Item => {
                self.flush();
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some(ListState::Ordered(next)) => {
                        let marker = format!("{next}. ");
                        *next = next.saturating_add(1);
                        marker
                    }
                    _ => "• ".to_owned(),
                };
                let marker = format!("{}{marker}", "  ".repeat(depth));
                self.hanging_indents
                    .push(UnicodeWidthStr::width(marker.as_str()));
                self.push(&marker, self.accent);
            }
            Tag::CodeBlock(kind) => {
                self.start_block();
                let language = match kind {
                    CodeBlockKind::Fenced(value) => bounded_label(&safe_text(&value)),
                    CodeBlockKind::Indented => None,
                };
                self.push("╭─ CODE", self.accent);
                if let Some(language) = language.as_deref() {
                    self.push(" · ", self.muted);
                    self.push(language, self.accent);
                }
                self.flush();
                self.code_block = true;
                self.code_highlighter = language
                    .as_deref()
                    .and_then(|value| CodeHighlighter::new(value, self.syntax));
                self.block_style = self.muted;
            }
            Tag::Table(alignments) => {
                self.start_block();
                self.table = Some(TableBuilder::new(alignments));
            }
            Tag::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.start_row(true);
                }
            }
            Tag::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    table.start_row(false);
                }
            }
            Tag::TableCell => {
                if let Some(table) = self.table.as_mut() {
                    table.start_cell();
                }
            }
            Tag::Link { dest_url, .. } => {
                self.link = Some(LinkState {
                    destination: safe_text(&dest_url),
                    label: String::new(),
                });
                self.inline_styles
                    .push(self.accent.add_modifier(Modifier::UNDERLINED));
            }
            Tag::Image { .. }
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
            TagEnd::Paragraph => self.flush(),
            TagEnd::Item => {
                self.flush();
                self.hanging_indents.pop();
            }
            TagEnd::TableCell => {
                if let Some(table) = self.table.as_mut() {
                    table.finish_cell();
                }
            }
            TagEnd::TableRow | TagEnd::TableHead => {
                self.table.as_mut().map(TableBuilder::finish_row);
            }
            TagEnd::Heading(_) => {
                self.flush();
                self.block_style = self.normal;
            }
            TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough => {
                self.inline_styles.pop();
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::CodeBlock => {
                self.flush();
                self.code_block = false;
                self.code_highlighter = None;
                self.block_style = self.normal;
                self.push("╰─", self.muted);
                self.flush();
            }
            TagEnd::Link => self.end_link(),
            TagEnd::Table => self.finish_table(),
            _ => {}
        }
    }

    fn text(&mut self, value: &str) {
        for (index, line) in value.split('\n').enumerate() {
            if index > 0 {
                self.flush();
            }
            self.record_link_label(line);
            if self.code_block {
                let fallback_style = self.current_style();
                let spans = self.code_highlighter.as_mut().map_or_else(
                    || vec![Span::styled(line.to_owned(), fallback_style)],
                    |highlighter| highlighter.highlight_line(line),
                );
                self.spans.extend(code_display_spans(
                    spans,
                    self.code_content_width(),
                    self.muted,
                ));
            } else {
                self.push(line, self.current_style());
            }
        }
    }

    fn code_content_width(&self) -> usize {
        self.width
            .saturating_sub(UnicodeWidthStr::width(self.prefix))
            .saturating_sub(self.quote_depth.saturating_mul(2))
            .saturating_sub(2)
    }

    fn current_style(&self) -> Style {
        self.inline_styles
            .iter()
            .fold(self.block_style, |style, layer| style.patch(*layer))
    }

    fn push_inline(&mut self, modifier: Modifier) {
        self.inline_styles
            .push(Style::default().add_modifier(modifier));
    }

    fn record_link_label(&mut self, value: &str) {
        if let Some(link) = self.link.as_mut() {
            link.label.push_str(value);
        }
    }

    fn end_link(&mut self) {
        self.inline_styles.pop();
        let Some(link) = self.link.take() else {
            return;
        };
        let Some(destination) = bounded_destination(&link.destination) else {
            return;
        };
        if destination == link.label.trim() {
            return;
        }
        self.push(" (", self.muted);
        self.push(&destination, self.accent.add_modifier(Modifier::UNDERLINED));
        self.push(")", self.muted);
    }

    fn push(&mut self, value: &str, style: Style) {
        if !value.is_empty() {
            let span = Span::styled(value.to_owned(), style);
            if self
                .table
                .as_mut()
                .is_none_or(|table| !table.push(span.clone()))
            {
                self.spans.push(span);
            }
        }
    }

    fn start_block(&mut self) {
        self.flush();
        if self.lists.is_empty()
            && self.quote_depth == 0
            && self.lines.last().is_some_and(|line| !line.spans.is_empty())
        {
            self.lines.push(Line::default());
        }
    }

    fn flush(&mut self) {
        let current_style = self.current_style();
        if self
            .table
            .as_mut()
            .is_some_and(|table| table.soft_break(current_style))
        {
            return;
        }
        if self.spans.is_empty() {
            return;
        }
        let mut lead = Vec::new();
        if !self.prefix.is_empty() {
            lead.push(Span::raw(self.prefix.to_owned()));
        }
        if self.quote_depth > 0 {
            lead.push(Span::styled("│ ".repeat(self.quote_depth), self.muted));
        }
        if self.code_block {
            lead.push(Span::styled("│ ", self.muted));
        }
        let lead_width = lead.iter().map(|span| span.width()).sum::<usize>();
        let content_width = self.width.saturating_sub(lead_width).max(1);
        let hanging = self
            .hanging_indents
            .last()
            .copied()
            .unwrap_or(0)
            .min(content_width.saturating_sub(1));
        let rows = wrap_styled(&self.spans, content_width, hanging);
        self.spans.clear();
        for (index, mut row) in rows.into_iter().enumerate() {
            let mut spans = lead.clone();
            if index > 0 && hanging > 0 {
                spans.push(Span::raw(" ".repeat(hanging)));
            }
            spans.append(&mut row);
            self.lines.push(Line::from(spans));
        }
    }

    fn finish_table(&mut self) {
        let Some(table) = self.table.take() else {
            return;
        };
        self.lines.extend(table.render(
            self.prefix,
            self.quote_depth,
            self.normal,
            self.accent,
            self.muted,
            self.width,
        ));
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

fn heading_style(level: HeadingLevel, accent: Style) -> Style {
    match level {
        HeadingLevel::H1 => accent.add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        HeadingLevel::H2 => accent.add_modifier(Modifier::BOLD),
        _ => accent.add_modifier(Modifier::ITALIC),
    }
}

fn bounded_label(value: &str) -> Option<String> {
    let value = value.split_whitespace().next()?.trim();
    (!value.is_empty()).then(|| value.chars().take(24).collect())
}

fn bounded_destination(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let mut chars = value.chars();
    let mut bounded = chars.by_ref().take(120).collect::<String>();
    if chars.next().is_some() {
        bounded.push('…');
    }
    Some(bounded)
}

fn code_display_spans(
    spans: Vec<Span<'static>>,
    width: usize,
    ellipsis_style: Style,
) -> Vec<Span<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let available = width.saturating_sub(1);
    let total = spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.replace('\t', "    ").as_str()))
        .sum::<usize>();
    let limit = if total > width { available } else { width };
    let mut rendered = Vec::new();
    let mut used = 0_usize;
    'spans: for span in spans {
        let expanded = span.content.replace('\t', "    ");
        let mut content = String::new();
        for grapheme in expanded.graphemes(true) {
            let grapheme_width = UnicodeWidthStr::width(grapheme);
            if used.saturating_add(grapheme_width) > limit {
                if !content.is_empty() {
                    rendered.push(Span::styled(content, span.style));
                }
                break 'spans;
            }
            content.push_str(grapheme);
            used = used.saturating_add(grapheme_width);
        }
        if !content.is_empty() {
            rendered.push(Span::styled(content, span.style));
        }
    }
    if total > width {
        rendered.push(Span::styled("…", ellipsis_style));
    }
    rendered
}

#[cfg(test)]
mod tests;
