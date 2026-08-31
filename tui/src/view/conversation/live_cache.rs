//! Incremental Markdown cache for the ephemeral live answer.

use ratatui::text::Line;

use crate::{application::LiveAnswer, Theme};

use super::super::{markdown::render_markdown, markdown_syntax::SyntaxPalette, palette};

#[derive(Default)]
pub(in crate::view) struct LiveRenderCache {
    stream_id: String,
    stable_source: String,
    width: u16,
    theme: u8,
    stable_lines: Vec<Line<'static>>,
    #[cfg(test)]
    stable_parses: usize,
    #[cfg(test)]
    tail_parses: usize,
}

impl LiveRenderCache {
    pub(in crate::view) fn render_markdown(
        &mut self,
        answer: &LiveAnswer,
        theme: Theme,
        width: u16,
    ) -> Vec<Line<'static>> {
        let colors = palette(theme);
        let theme_key = theme_key(theme);
        let stable = answer.markdown.stable_prefix();
        let identity_changed = self.stream_id != answer.key.stream_id
            || self.width != width
            || self.theme != theme_key;
        if identity_changed || self.stable_source != stable {
            self.stream_id.clone_from(&answer.key.stream_id);
            self.width = width;
            self.theme = theme_key;
            self.stable_source.clear();
            self.stable_source.push_str(stable);
            self.stable_lines = if stable.is_empty() {
                Vec::new()
            } else {
                #[cfg(test)]
                {
                    self.stable_parses += 1;
                }
                render_source(stable, colors, width)
            };
        }
        let mut lines = self.stable_lines.clone();
        let tail = answer.markdown.mutable_tail();
        if !tail.is_empty() {
            #[cfg(test)]
            {
                self.tail_parses += 1;
            }
            lines.extend(render_source(tail, colors, width));
        }
        lines
    }

    pub(super) fn retain_for(&mut self, answer: Option<&LiveAnswer>) {
        if answer.is_some() {
            return;
        }
        self.stream_id.clear();
        self.stable_source.clear();
        self.stable_lines.clear();
        self.width = 0;
        self.theme = 0;
    }
}

fn render_source(
    source: &str,
    colors: super::super::style::Palette,
    width: u16,
) -> Vec<Line<'static>> {
    render_markdown(
        source,
        "  ",
        colors.normal,
        colors.agent,
        colors.muted,
        SyntaxPalette::from_palette(colors),
        width,
    )
}

fn theme_key(theme: Theme) -> u8 {
    match theme {
        Theme::System => 0,
        Theme::Dark => 1,
        Theme::Light => 2,
        Theme::Mono => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{LiveAnswerExpectation, LiveAnswerProjection};
    use garive_host_client::{LiveOutputEvent, LiveOutputEventKind};

    #[test]
    fn tail_delta_reuses_stable_parse_and_preserves_monolithic_semantics() {
        let mut projection = live_projection("Stable **Markdown**.\n\nMutable tail");
        let mut cache = LiveRenderCache::default();
        let first = cache.render_markdown(projection.current().unwrap(), Theme::Dark, 80);
        assert_eq!((cache.stable_parses, cache.tail_parses), (1, 1));
        assert_eq!(
            first,
            render_source(
                &projection.current().unwrap().presented_text,
                palette(Theme::Dark),
                80,
            )
        );

        projection.apply(
            event(
                2,
                LiveOutputEventKind::TextDelta {
                    text: " grows".into(),
                },
            ),
            expectation(),
        );
        projection.advance_frame(false);
        let second = cache.render_markdown(projection.current().unwrap(), Theme::Dark, 80);

        assert_eq!((cache.stable_parses, cache.tail_parses), (1, 2));
        assert_eq!(
            second,
            render_source(
                &projection.current().unwrap().presented_text,
                palette(Theme::Dark),
                80,
            )
        );
    }

    #[test]
    fn resize_reflows_all_markdown_and_takeover_clears_entry() {
        let mut projection = live_projection("Stable paragraph.\n\nMutable tail");
        let mut cache = LiveRenderCache::default();
        let _ = cache.render_markdown(projection.current().unwrap(), Theme::Mono, 80);
        let _ = cache.render_markdown(projection.current().unwrap(), Theme::Mono, 40);
        assert_eq!((cache.stable_parses, cache.tail_parses), (2, 2));

        projection.durable_takeover("session-a", "turn-a", Some("execution-a"));
        cache.retain_for(projection.current());
        assert!(cache.stream_id.is_empty() && cache.stable_lines.is_empty());
    }

    fn live_projection(text: &str) -> LiveAnswerProjection {
        let mut projection = LiveAnswerProjection::default();
        projection.apply(
            event(
                1,
                LiveOutputEventKind::Snapshot {
                    text: text.into(),
                    through_sequence: 1,
                },
            ),
            expectation(),
        );
        projection
    }

    fn event(sequence: u64, kind: LiveOutputEventKind) -> LiveOutputEvent {
        LiveOutputEvent {
            api_version: "v1".into(),
            session_id: "session-a".into(),
            turn_id: "turn-a".into(),
            execution_id: "execution-a".into(),
            stream_id: "00000000-0000-4000-8000-000000000001".into(),
            sequence,
            kind,
        }
    }

    fn expectation() -> LiveAnswerExpectation<'static> {
        LiveAnswerExpectation {
            selected_session: "session-a",
            active_turn: Some("turn-a"),
            active_execution: Some("execution-a"),
            detached: false,
        }
    }
}
