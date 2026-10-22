//! The terminal around the TUI: ratatui's raw mode and alternate screen,
//! plus bracketed paste (blueprint 35, "Terminal and browser keys"). Every
//! way out restores all of it: quit, an error, a handoff to another
//! program, and a panic.

use ratatui::crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use ratatui::crossterm::execute;
use std::io::stdout;
use std::sync::Once;

pub(crate) fn init() -> ratatui::DefaultTerminal {
    let terminal = ratatui::init();
    install_panic_hook();
    // Pasted text arrives as one event instead of keystrokes. A terminal
    // without bracketed paste ignores the sequence.
    let _ = execute!(stdout(), EnableBracketedPaste);
    terminal
}

pub(crate) fn restore() {
    let _ = execute!(stdout(), DisableBracketedPaste);
    ratatui::restore();
}

/// Stop the process, as the shell's Ctrl-z would. Raw mode turns Ctrl-z
/// into a key, so the TUI raises SIGTSTP itself; the call returns when the
/// shell resumes the process with SIGCONT.
#[cfg(unix)]
pub(crate) fn suspend_process() {
    let _ = nix::sys::signal::raise(nix::sys::signal::Signal::SIGTSTP);
}

#[cfg(not(unix))]
pub(crate) fn suspend_process() {}

/// Restores the terminal when dropped, so an error that ends the run
/// early leaves the shell usable too.
pub(crate) struct RestoreOnDrop;

impl Drop for RestoreOnDrop {
    fn drop(&mut self) {
        restore();
    }
}

/// ratatui's own hook restores raw mode and the screen; this one, run
/// before it, turns off what `init` added. Installed once, since every
/// re-init after a handoff would otherwise add another layer.
fn install_panic_hook() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = execute!(stdout(), DisableBracketedPaste);
            previous(info);
        }));
    });
}
