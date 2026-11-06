//! The moved-key notices this version ignores (blueprint 35, D171).
//!
//! `cargo run -p mxr-keymap --example expired_key_moves` prints one GitHub
//! warning annotation per expired `[[action.moved]]` entry and exits 0: an
//! expired entry does nothing, so CI only points at it. With `--prune` it
//! deletes them from `crates/keymap/keymap.toml` instead
//! (`scripts/prune_expired_key_moves` then regenerates the generated files).

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("keymap.toml");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("can't read {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    // Read at run time, not the compiled-in copy, so a prune and a second
    // run see the file as it is now.
    let keymap = match mxr_keymap::Keymap::parse(&text) {
        Ok(keymap) => keymap,
        Err(error) => {
            eprintln!("{} doesn't parse: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let expired = keymap.expired_moves(mxr_keymap::VERSION);
    if std::env::args().any(|arg| arg == "--prune") {
        if expired.is_empty() {
            println!("No moved key has expired at {}.", mxr_keymap::VERSION);
            return ExitCode::SUCCESS;
        }
        let pruned = match mxr_keymap::prune_expired_moves(&text, mxr_keymap::VERSION) {
            Ok(pruned) => pruned,
            Err(error) => {
                eprintln!("{} doesn't parse: {error}", path.display());
                return ExitCode::FAILURE;
            }
        };
        if let Err(error) = std::fs::write(&path, pruned) {
            eprintln!("can't write {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
        for line in &expired {
            println!("deleted {line}");
        }
        return ExitCode::SUCCESS;
    }
    for line in &expired {
        println!(
            "::warning file=crates/keymap/keymap.toml,title=Expired key move::{line}; mxr ignores it, and the weekly prune deletes it (scripts/prune_expired_key_moves)"
        );
    }
    ExitCode::SUCCESS
}
