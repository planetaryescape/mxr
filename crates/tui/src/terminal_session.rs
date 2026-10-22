//! The terminal around the TUI: ratatui's raw mode and alternate screen,
//! plus bracketed paste and, where the terminal supports it, unambiguous
//! escape codes (blueprint 35, "Terminal and browser keys"). Every way out
//! restores all of it: quit, an error, a handoff to another program, and a
//! panic.

use ratatui::crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use std::io::{self, stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Once, OnceLock};

/// The flags were pushed and must be popped on the way out.
static ENHANCED: AtomicBool = AtomicBool::new(false);

pub(crate) fn init() -> ratatui::DefaultTerminal {
    let terminal = ratatui::init();
    install_panic_hook();
    // Pasted text arrives as one event instead of keystrokes. A terminal
    // without bracketed paste ignores the sequence.
    let _ = execute!(stdout(), EnableBracketedPaste);
    // Esc then arrives at once instead of waiting to see whether it starts
    // an Alt sequence. The query can block until it times out on a terminal
    // that never answers, so it runs once, here, where init already talks
    // to the terminal; a resume reuses the answer. No key depends on it.
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    let pushed = push_enhancement(
        || {
            Ok(*SUPPORTED.get_or_init(|| {
                ratatui::crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false)
            }))
        },
        || {
            execute!(
                stdout(),
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )
        },
    );
    ENHANCED.store(pushed, Ordering::SeqCst);
    terminal
}

/// Push the flags only when the terminal says it supports them; an error
/// from either step reads as unsupported. True when they were pushed.
fn push_enhancement(
    supported: impl FnOnce() -> io::Result<bool>,
    push: impl FnOnce() -> io::Result<()>,
) -> bool {
    matches!(supported(), Ok(true)) && push().is_ok()
}

/// Undo what `init` added to ratatui's setup.
fn disable_extras() {
    if ENHANCED.swap(false, Ordering::SeqCst) {
        let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
    }
    let _ = execute!(stdout(), DisableBracketedPaste);
}

pub(crate) fn restore() {
    disable_extras();
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
            disable_extras();
            previous(info);
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::push_enhancement;
    use std::cell::Cell;
    use std::io;

    #[test]
    fn enhancement_is_pushed_only_when_the_terminal_says_yes() {
        let pushes = Cell::new(0);
        let push = || {
            pushes.set(pushes.get() + 1);
            Ok(())
        };
        assert!(push_enhancement(|| Ok(true), push));
        assert!(!push_enhancement(|| Ok(false), push));
        assert!(!push_enhancement(
            || Err(io::Error::new(io::ErrorKind::TimedOut, "no answer")),
            push
        ));
        assert_eq!(pushes.get(), 1, "pushed without a yes");
        // A failed push is not recorded, so the way out pops nothing.
        assert!(!push_enhancement(
            || Ok(true),
            || Err(io::Error::other("closed"))
        ));
    }
}
