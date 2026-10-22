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
