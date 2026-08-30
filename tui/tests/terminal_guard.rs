#![allow(dead_code, unused_imports)]

use std::{cell::RefCell, io, rc::Rc};

#[path = "../src/runtime/terminal.rs"]
mod terminal;

use terminal::{TerminalError, TerminalGuard, TerminalOps, TerminalOptions};

#[derive(Clone)]
struct FakeOps {
    calls: Rc<RefCell<Vec<&'static str>>>,
    fail_at: Option<&'static str>,
    available: bool,
}

impl FakeOps {
    fn call(&mut self, name: &'static str) -> io::Result<()> {
        self.calls.borrow_mut().push(name);
        if self.fail_at == Some(name) {
            Err(io::Error::other("injected"))
        } else {
            Ok(())
        }
    }
}

impl TerminalOps for FakeOps {
    fn terminals_available(&self) -> bool {
        self.available
    }
    fn enable_raw(&mut self) -> io::Result<()> {
        self.call("enable_raw")
    }
    fn disable_raw(&mut self) -> io::Result<()> {
        self.call("disable_raw")
    }
    fn enter_alternate_screen(&mut self) -> io::Result<()> {
        self.call("enter_alt")
    }
    fn leave_alternate_screen(&mut self) -> io::Result<()> {
        self.call("leave_alt")
    }
    fn enable_paste(&mut self) -> io::Result<()> {
        self.call("enable_paste")
    }
    fn disable_paste(&mut self) -> io::Result<()> {
        self.call("disable_paste")
    }
    fn enable_focus(&mut self) -> io::Result<()> {
        self.call("enable_focus")
    }
    fn disable_focus(&mut self) -> io::Result<()> {
        self.call("disable_focus")
    }
    fn enable_mouse(&mut self) -> io::Result<()> {
        self.call("enable_mouse")
    }
    fn disable_mouse(&mut self) -> io::Result<()> {
        self.call("disable_mouse")
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        self.call("show_cursor")
    }
    fn flush(&mut self) -> io::Result<()> {
        self.call("flush")
    }
}

fn fake(fail_at: Option<&'static str>) -> (FakeOps, Rc<RefCell<Vec<&'static str>>>) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    (
        FakeOps {
            calls: calls.clone(),
            fail_at,
            available: true,
        },
        calls,
    )
}

#[test]
fn normal_restore_is_reverse_order_and_idempotent() {
    let (ops, calls) = fake(None);
    let mut guard = TerminalGuard::acquire(ops, TerminalOptions::default()).unwrap();
    guard.restore().unwrap();
    guard.restore().unwrap();
    drop(guard);
    assert_eq!(
        *calls.borrow(),
        [
            "enable_raw",
            "enter_alt",
            "enable_paste",
            "enable_focus",
            "disable_focus",
            "disable_paste",
            "show_cursor",
            "leave_alt",
            "disable_raw",
            "flush"
        ]
    );
}

#[test]
fn every_setup_failure_rolls_back_only_completed_steps() {
    for failure in ["enter_alt", "enable_paste", "enable_focus"] {
        let (ops, calls) = fake(Some(failure));
        assert_eq!(
            TerminalGuard::acquire(ops, TerminalOptions::default()).err(),
            Some(TerminalError::Setup)
        );
        let calls = calls.borrow();
        assert!(calls.contains(&"show_cursor"));
        assert!(calls.contains(&"disable_raw"));
        assert_eq!(calls.last(), Some(&"flush"));
    }
}

#[test]
fn raw_mode_failure_performs_only_safe_finalization() {
    let (ops, calls) = fake(Some("enable_raw"));
    assert_eq!(
        TerminalGuard::acquire(ops, TerminalOptions::default()).err(),
        Some(TerminalError::Setup)
    );
    assert_eq!(*calls.borrow(), ["enable_raw", "show_cursor", "flush"]);
}

#[test]
fn screen_reader_mode_never_enters_the_alternate_screen() {
    let (ops, calls) = fake(None);
    let guard = TerminalGuard::acquire(
        ops,
        TerminalOptions {
            screen_reader: true,
            mouse: false,
        },
    )
    .unwrap();
    drop(guard);
    assert!(!calls.borrow().contains(&"enter_alt"));
    assert!(!calls.borrow().contains(&"leave_alt"));
}

#[test]
fn unwinding_a_panic_restores_every_acquired_mode() {
    let (ops, calls) = fake(None);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = TerminalGuard::acquire(
            ops,
            TerminalOptions {
                screen_reader: false,
                mouse: true,
            },
        )
        .unwrap();
        panic!("injected after acquisition");
    }));
    assert!(result.is_err());
    assert_eq!(
        *calls.borrow(),
        [
            "enable_raw",
            "enter_alt",
            "enable_paste",
            "enable_focus",
            "enable_mouse",
            "disable_mouse",
            "disable_focus",
            "disable_paste",
            "show_cursor",
            "leave_alt",
            "disable_raw",
            "flush",
        ]
    );
}

#[test]
fn mouse_capture_is_opt_in_and_restored_before_focus() {
    let (ops, calls) = fake(None);
    let guard = TerminalGuard::acquire(
        ops,
        TerminalOptions {
            screen_reader: false,
            mouse: true,
        },
    )
    .unwrap();
    drop(guard);
    let calls = calls.borrow();
    assert!(calls.contains(&"enable_mouse"));
    let mouse = calls
        .iter()
        .position(|value| *value == "disable_mouse")
        .unwrap();
    let focus = calls
        .iter()
        .position(|value| *value == "disable_focus")
        .unwrap();
    assert!(mouse < focus);
}

#[test]
fn non_terminal_input_is_rejected_before_mutation() {
    let (mut ops, calls) = fake(None);
    ops.available = false;
    assert_eq!(
        TerminalGuard::acquire(ops, TerminalOptions::default()).err(),
        Some(TerminalError::NotATerminal)
    );
    assert!(calls.borrow().is_empty());
}
