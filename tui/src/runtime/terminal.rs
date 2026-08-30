use std::io::{self, IsTerminal, Stderr, Write};

use crossterm::{
    cursor::Show,
    event::{
        DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TerminalOptions {
    pub(crate) screen_reader: bool,
    pub(crate) mouse: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalError {
    NotATerminal,
    Setup,
    Restore,
}

pub(crate) trait TerminalOps {
    fn terminals_available(&self) -> bool;
    fn enable_raw(&mut self) -> io::Result<()>;
    fn disable_raw(&mut self) -> io::Result<()>;
    fn enter_alternate_screen(&mut self) -> io::Result<()>;
    fn leave_alternate_screen(&mut self) -> io::Result<()>;
    fn enable_paste(&mut self) -> io::Result<()>;
    fn disable_paste(&mut self) -> io::Result<()>;
    fn enable_focus(&mut self) -> io::Result<()>;
    fn disable_focus(&mut self) -> io::Result<()>;
    fn enable_mouse(&mut self) -> io::Result<()>;
    fn disable_mouse(&mut self) -> io::Result<()>;
    fn set_title(&mut self, title: &str) -> io::Result<()>;
    fn show_cursor(&mut self) -> io::Result<()>;
    fn flush(&mut self) -> io::Result<()>;
}

pub(crate) struct TerminalGuard<O: TerminalOps> {
    ops: O,
    raw: bool,
    alternate: bool,
    paste: bool,
    focus: bool,
    mouse: bool,
    current_title: Option<String>,
    restored: bool,
}

impl<O: TerminalOps> TerminalGuard<O> {
    pub(crate) fn acquire(ops: O, options: TerminalOptions) -> Result<Self, TerminalError> {
        if !ops.terminals_available() {
            return Err(TerminalError::NotATerminal);
        }
        let mut guard = Self {
            ops,
            raw: false,
            alternate: false,
            paste: false,
            focus: false,
            mouse: false,
            current_title: None,
            restored: false,
        };
        if guard.ops.enable_raw().is_err() {
            return Err(TerminalError::Setup);
        }
        guard.raw = true;
        if !options.screen_reader {
            guard.step(|ops| ops.enter_alternate_screen())?;
            guard.alternate = true;
        }
        guard.step(|ops| ops.enable_paste())?;
        guard.paste = true;
        guard.step(|ops| ops.enable_focus())?;
        guard.focus = true;
        if options.mouse {
            guard.step(|ops| ops.enable_mouse())?;
            guard.mouse = true;
        }
        Ok(guard)
    }

    pub(crate) fn set_title(&mut self, title: &str) -> Result<(), TerminalError> {
        if self.current_title.as_deref() == Some(title) {
            return Ok(());
        }
        self.ops
            .set_title(title)
            .map_err(|_| TerminalError::Setup)?;
        self.current_title = Some(title.to_owned());
        Ok(())
    }

    pub(crate) fn restore(&mut self) -> Result<(), TerminalError> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        let mut failed = false;
        if self.current_title.take().is_some() {
            failed |= self.ops.set_title("Garive").is_err();
        }
        if self.mouse {
            failed |= self.ops.disable_mouse().is_err();
            self.mouse = false;
        }
        if self.focus {
            failed |= self.ops.disable_focus().is_err();
            self.focus = false;
        }
        if self.paste {
            failed |= self.ops.disable_paste().is_err();
            self.paste = false;
        }
        failed |= self.ops.show_cursor().is_err();
        if self.alternate {
            failed |= self.ops.leave_alternate_screen().is_err();
            self.alternate = false;
        }
        if self.raw {
            failed |= self.ops.disable_raw().is_err();
            self.raw = false;
        }
        failed |= self.ops.flush().is_err();
        if failed {
            Err(TerminalError::Restore)
        } else {
            Ok(())
        }
    }

    fn step(
        &mut self,
        operation: impl FnOnce(&mut O) -> io::Result<()>,
    ) -> Result<(), TerminalError> {
        if operation(&mut self.ops).is_err() {
            let _ = self.restore();
            Err(TerminalError::Setup)
        } else {
            Ok(())
        }
    }
}

impl<O: TerminalOps> Drop for TerminalGuard<O> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

pub(crate) struct SystemTerminal {
    stderr: Stderr,
}

impl Default for SystemTerminal {
    fn default() -> Self {
        Self {
            stderr: io::stderr(),
        }
    }
}

impl TerminalOps for SystemTerminal {
    fn terminals_available(&self) -> bool {
        io::stdin().is_terminal() && self.stderr.is_terminal()
    }

    fn enable_raw(&mut self) -> io::Result<()> {
        enable_raw_mode()
    }
    fn disable_raw(&mut self) -> io::Result<()> {
        disable_raw_mode()
    }
    fn enter_alternate_screen(&mut self) -> io::Result<()> {
        execute!(self.stderr, EnterAlternateScreen).map(|_| ())
    }
    fn leave_alternate_screen(&mut self) -> io::Result<()> {
        execute!(self.stderr, LeaveAlternateScreen).map(|_| ())
    }
    fn enable_paste(&mut self) -> io::Result<()> {
        execute!(self.stderr, EnableBracketedPaste).map(|_| ())
    }
    fn disable_paste(&mut self) -> io::Result<()> {
        execute!(self.stderr, DisableBracketedPaste).map(|_| ())
    }
    fn enable_focus(&mut self) -> io::Result<()> {
        execute!(self.stderr, EnableFocusChange).map(|_| ())
    }
    fn disable_focus(&mut self) -> io::Result<()> {
        execute!(self.stderr, DisableFocusChange).map(|_| ())
    }
    fn enable_mouse(&mut self) -> io::Result<()> {
        execute!(self.stderr, EnableMouseCapture).map(|_| ())
    }
    fn disable_mouse(&mut self) -> io::Result<()> {
        execute!(self.stderr, DisableMouseCapture).map(|_| ())
    }
    fn set_title(&mut self, title: &str) -> io::Result<()> {
        execute!(self.stderr, SetTitle(title)).map(|_| ())
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        execute!(self.stderr, Show).map(|_| ())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stderr.flush()
    }
}
