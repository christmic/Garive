use crate::Theme;

pub(super) const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DefaultColors {
    pub(super) foreground: (u8, u8, u8),
    pub(super) background: (u8, u8, u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalTheme {
    system: Theme,
}

impl TerminalTheme {
    pub(super) fn from_default_colors(colors: Option<DefaultColors>) -> Self {
        let system = colors
            .map(|colors| {
                if is_light(colors.background) {
                    Theme::Light
                } else {
                    Theme::Dark
                }
            })
            .unwrap_or(Theme::Dark);
        Self { system }
    }

    pub(super) fn resolve(self, selected: Theme) -> Theme {
        match selected {
            Theme::System => self.system,
            explicit => explicit,
        }
    }
}

impl Default for TerminalTheme {
    fn default() -> Self {
        Self::from_default_colors(None)
    }
}

pub(super) fn probe(timeout: std::time::Duration) -> TerminalTheme {
    #[cfg(unix)]
    let colors = unix::default_colors(timeout).ok().flatten();
    #[cfg(not(unix))]
    let colors = None;
    TerminalTheme::from_default_colors(colors)
}

fn is_light(background: (u8, u8, u8)) -> bool {
    let (red, green, blue) = background;
    let luminance = 0.299 * f32::from(red) + 0.587 * f32::from(green) + 0.114 * f32::from(blue);
    luminance > 128.0
}

fn parse_default_colors(buffer: &[u8]) -> Option<DefaultColors> {
    Some(DefaultColors {
        foreground: parse_osc_color(buffer, 10)?,
        background: parse_osc_color(buffer, 11)?,
    })
}

fn parse_osc_color(buffer: &[u8], slot: u8) -> Option<(u8, u8, u8)> {
    let prefix = format!("\x1b]{slot};");
    let start = buffer
        .windows(prefix.len())
        .position(|window| window == prefix.as_bytes())?;
    let rest = &buffer[start + prefix.len()..];
    let end = rest
        .iter()
        .enumerate()
        .find_map(|(index, byte)| match byte {
            0x07 => Some(index),
            0x1b if rest.get(index + 1) == Some(&b'\\') => Some(index),
            _ => None,
        })?;
    parse_osc_rgb(std::str::from_utf8(&rest[..end]).ok()?)
}

fn parse_osc_rgb(payload: &str) -> Option<(u8, u8, u8)> {
    let (kind, values) = payload.trim().split_once(':')?;
    if !kind.eq_ignore_ascii_case("rgb") && !kind.eq_ignore_ascii_case("rgba") {
        return None;
    }
    let mut components = values.split('/');
    let red = parse_component(components.next()?)?;
    let green = parse_component(components.next()?)?;
    let blue = parse_component(components.next()?)?;
    if kind.eq_ignore_ascii_case("rgba") {
        parse_component(components.next()?)?;
    }
    components.next().is_none().then_some((red, green, blue))
}

fn parse_component(component: &str) -> Option<u8> {
    match component.len() {
        2 => u8::from_str_radix(component, 16).ok(),
        4 => u16::from_str_radix(component, 16)
            .ok()
            .map(|value| (value / 257) as u8),
        _ => None,
    }
}

#[cfg(unix)]
mod unix {
    use std::{
        fs::{File, OpenOptions},
        io::{self, Read, Write},
        thread,
        time::{Duration, Instant},
    };

    use rustix::fs::{fcntl_getfl, fcntl_setfl, OFlags};

    use super::{parse_default_colors, DefaultColors};

    const MAX_RESPONSE_BYTES: usize = 4_096;

    struct ProbeTty {
        reader: File,
        writer: File,
        original_flags: OFlags,
    }

    impl ProbeTty {
        fn open() -> io::Result<Self> {
            let stdin = io::stdin();
            let stderr = io::stderr();
            match (rustix::io::dup(&stdin), rustix::io::dup(&stderr)) {
                (Ok(reader), Ok(writer)) => Self::new(reader.into(), writer.into()),
                _ => Self::new(
                    OpenOptions::new().read(true).open("/dev/tty")?,
                    OpenOptions::new().write(true).open("/dev/tty")?,
                ),
            }
        }

        fn new(reader: File, writer: File) -> io::Result<Self> {
            let original_flags = fcntl_getfl(&reader).map_err(io::Error::from)?;
            fcntl_setfl(&reader, original_flags | OFlags::NONBLOCK).map_err(io::Error::from)?;
            Ok(Self {
                reader,
                writer,
                original_flags,
            })
        }

        fn read_available(&mut self, buffer: &mut Vec<u8>) -> io::Result<()> {
            let mut chunk = [0_u8; 256];
            loop {
                match self.reader.read(&mut chunk) {
                    Ok(0) => return Ok(()),
                    Ok(count) => {
                        let remaining = MAX_RESPONSE_BYTES.saturating_sub(buffer.len());
                        if remaining == 0 {
                            return Err(io::Error::other("terminal response exceeded bound"));
                        }
                        buffer.extend_from_slice(&chunk[..count.min(remaining)]);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            }
        }
    }

    impl Drop for ProbeTty {
        fn drop(&mut self) {
            let _ = fcntl_setfl(&self.reader, self.original_flags);
        }
    }

    pub(super) fn default_colors(timeout: Duration) -> io::Result<Option<DefaultColors>> {
        let mut tty = ProbeTty::open()?;
        tty.writer.write_all(b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\")?;
        tty.writer.flush()?;
        let deadline = Instant::now() + timeout;
        let mut response = Vec::new();
        loop {
            tty.read_available(&mut response)?;
            if let Some(colors) = parse_default_colors(&response) {
                return Ok(Some(colors));
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            thread::sleep(
                deadline
                    .saturating_duration_since(now)
                    .min(Duration::from_millis(2)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_paired_bel_and_st_responses_in_either_order() {
        let colors = DefaultColors {
            foreground: (238, 238, 238),
            background: (17, 17, 17),
        };
        assert_eq!(
            parse_default_colors(
                b"typed\x1b]10;rgb:eeee/eeee/eeee\x1b\\noise\x1b]11;rgb:1111/1111/1111\x07"
            ),
            Some(colors)
        );
        assert_eq!(
            parse_default_colors(b"\x1b]11;rgb:11/11/11\x07\x1b]10;rgba:ee/ee/ee/ff\x1b\\"),
            Some(colors)
        );
    }

    #[test]
    fn rejects_partial_or_malformed_responses() {
        assert_eq!(
            parse_default_colors(b"\x1b]10;rgb:eeee/eeee/eeee\x1b\\"),
            None
        );
        assert_eq!(
            parse_default_colors(b"\x1b]10;rgb:ee/ee/ee\x07\x1b]11;rgb:11/11/11/11\x07"),
            None
        );
    }

    #[test]
    fn resolves_system_from_background_and_preserves_explicit_themes() {
        let light = TerminalTheme::from_default_colors(Some(DefaultColors {
            foreground: (0, 0, 0),
            background: (245, 245, 245),
        }));
        assert_eq!(light.resolve(Theme::System), Theme::Light);
        assert_eq!(light.resolve(Theme::Mono), Theme::Mono);
        assert_eq!(TerminalTheme::default().resolve(Theme::System), Theme::Dark);
    }
}
