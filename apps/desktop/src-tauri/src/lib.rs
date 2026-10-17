//! mxr desktop: a window around the local mxr web app.
//!
//! The daemon is the system. This shell runs `mxr web --print-url --no-open`,
//! loads the URL it prints and adds a global new-message shortcut. It bundles
//! no frontend (only a down page), manages no daemon and has no updater.
//!
//! Security: the shell grants no Tauri IPC to anything. There are no
//! capabilities and no `invoke` handler, so neither the bundled down page
//! nor the bridge origin can call Rust. "Try again" is a navigation that
//! `on_navigation` intercepts.

mod mxr_cli;
mod navigation;

use mxr_cli::LaunchError;
use navigation::{classify, may_open_externally, Nav, DOWN_PAGE};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{
    AppHandle, Manager, RunEvent, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_opener::OpenerExt;

const MAIN: &str = "main";
/// Midnight desk, so the window never flashes white before the page paints.
const DESK: tauri::window::Color = tauri::window::Color(0x0d, 0x16, 0x1f, 0xff);
/// Overrides the default shortcut, e.g. `MXR_DESKTOP_SHORTCUT=super+shift+m`.
const SHORTCUT_ENV: &str = "MXR_DESKTOP_SHORTCUT";

#[derive(Default)]
struct Shell {
    /// Origin the window may show, set once `mxr web` has printed it.
    bridge: Mutex<Option<Url>>,
    launching: AtomicBool,
}

impl Shell {
    fn bridge(&self) -> Option<Url> {
        self.bridge.lock().ok().and_then(|bridge| bridge.clone())
    }
}

/// Control+Option+Command+M. Command+Shift+M is already "Move to" in Mail and
/// "switch profile" in Chrome, and a global shortcut would steal it from every
/// app, so the default avoids the common bindings.
fn default_shortcut() -> Shortcut {
    Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER),
        Code::KeyM,
    )
}

fn configured_shortcut() -> Shortcut {
    std::env::var(SHORTCUT_ENV)
        .ok()
        .and_then(|value| Shortcut::from_str(value.trim()).ok())
        .unwrap_or_else(default_shortcut)
}

fn show_main(app: &AppHandle) -> Option<WebviewWindow> {
    let window = app.get_webview_window(MAIN)?;
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    Some(window)
}

fn open_new_message(app: &AppHandle) {
    let Some(window) = show_main(app) else { return };
    let shell = app.state::<Arc<Shell>>();
    // Only when the web app is showing; on the down page there is nothing to compose in.
    if let Some(target) = shell
        .bridge()
        .and_then(|bridge| bridge.join("compose/new").ok())
    {
        if window
            .url()
            .is_ok_and(|current| current.origin() == target.origin())
        {
            let _ = window.navigate(target);
        }
    }
}

fn down_url(reason: &str) -> Url {
    let mut url = Url::parse(DOWN_PAGE).expect("DOWN_PAGE is a valid URL");
    url.set_query(Some(&format!("reason={reason}")));
    url
}

/// Runs `mxr web` off the main thread, then points the window at the result.
fn start_web(app: &AppHandle) {
    let shell = app.state::<Arc<Shell>>();
    if shell.launching.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let shell = app.state::<Arc<Shell>>();
        let target = match mxr_cli::launch(mxr_cli::LAUNCH_TIMEOUT) {
            Ok(url) => {
                if let Ok(mut bridge) = shell.bridge.lock() {
                    *bridge = Some(url.clone());
                }
                url
            }
            Err(error) => {
                if let Ok(mut bridge) = shell.bridge.lock() {
                    *bridge = None;
                }
                down_url(LaunchError::reason(error))
            }
        };
        shell.launching.store(false, Ordering::SeqCst);
        if let Some(window) = app.get_webview_window(MAIN) {
            let _ = window.navigate(target);
        }
    });
}

fn open_externally(app: &AppHandle, url: &Url) {
    if may_open_externally(url) {
        let _ = app.opener().open_url(url.as_str(), None::<&str>);
    }
}

fn create_main_window(app: &AppHandle) -> tauri::Result<()> {
    let nav_app = app.clone();
    let popup_app = app.clone();
    WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("down.html".into()))
        .title("mxr")
        .inner_size(1280.0, 840.0)
        .min_inner_size(900.0, 600.0)
        .background_color(DESK)
        .on_navigation(move |url| {
            let shell = nav_app.state::<Arc<Shell>>();
            match classify(url, shell.bridge().as_ref()) {
                Nav::Allow => true,
                Nav::Retry => {
                    start_web(&nav_app);
                    false
                }
                Nav::External => {
                    open_externally(&nav_app, url);
                    false
                }
                Nav::Deny => false,
            }
        })
        // `target=_blank` and `window.open` never get a second window.
        .on_new_window(move |url, _features| {
            open_externally(&popup_app, &url);
            tauri::webview::NewWindowResponse::Deny
        })
        .build()?;
    Ok(())
}

pub fn run() {
    let shortcut = configured_shortcut();

    let app = tauri::Builder::default()
        // Must be registered first.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, pressed, event| {
                    if *pressed == shortcut && event.state() == ShortcutState::Pressed {
                        open_new_message(app);
                    }
                })
                .build(),
        )
        .manage(Arc::new(Shell::default()))
        .setup(move |app| {
            create_main_window(app.handle())?;
            // Taken by another app or denied: the window still works.
            let _ = app.global_shortcut().register(shortcut);
            start_web(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing hides the window so the shortcut and the Dock icon can
            // bring it back; Command+Q quits.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == MAIN {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build the mxr desktop app");

    app.run(|app, event| {
        if let RunEvent::Reopen { .. } = event {
            show_main(app);
        }
    });
}
