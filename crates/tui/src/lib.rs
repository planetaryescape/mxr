pub mod action;
pub mod app;
pub mod client;
pub mod input;
pub mod ui;

use app::App;
use client::Client;
use crossterm::event::{Event, EventStream};
use futures::StreamExt;

pub async fn run() -> anyhow::Result<()> {
    let socket_path = daemon_socket_path();
    let mut client = Client::connect(&socket_path).await?;

    let mut app = App::new();
    app.load(&mut client).await?;

    let mut terminal = ratatui::init();
    let mut events = EventStream::new();

    loop {
        terminal.draw(|frame| app.draw(frame))?;

        let timeout = if app.input_pending() {
            std::time::Duration::from_millis(500)
        } else {
            std::time::Duration::from_secs(60)
        };

        tokio::select! {
            event = events.next() => {
                if let Some(Ok(Event::Key(key))) = event {
                    if let Some(action) = app.handle_key(key) {
                        app.apply(action);
                    }
                }
            }
            _ = tokio::time::sleep(timeout) => {
                app.tick();
            }
        }

        if app.should_quit {
            break;
        }
    }

    ratatui::restore();
    Ok(())
}

fn daemon_socket_path() -> std::path::PathBuf {
    if cfg!(target_os = "macos") {
        dirs::home_dir()
            .unwrap()
            .join("Library/Application Support/mxr/mxr.sock")
    } else {
        dirs::runtime_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("mxr/mxr.sock")
    }
}

#[cfg(test)]
mod tests {
    use super::action::Action;
    use super::app::ActivePane;
    use super::app::App;
    use super::input::InputHandler;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use mxr_core::id::*;
    use mxr_core::types::*;

    fn make_test_envelopes(count: usize) -> Vec<Envelope> {
        (0..count)
            .map(|i| Envelope {
                id: MessageId::new(),
                account_id: AccountId::new(),
                provider_id: format!("fake-{}", i),
                thread_id: ThreadId::new(),
                message_id_header: None,
                in_reply_to: None,
                references: vec![],
                from: Address {
                    name: Some(format!("User {}", i)),
                    email: format!("user{}@example.com", i),
                },
                to: vec![],
                cc: vec![],
                bcc: vec![],
                subject: format!("Subject {}", i),
                date: chrono::Utc::now(),
                flags: if i % 2 == 0 {
                    MessageFlags::READ
                } else {
                    MessageFlags::empty()
                },
                snippet: format!("Snippet {}", i),
                has_attachments: false,
                size_bytes: 1000,
                unsubscribe: UnsubscribeMethod::None,
            })
            .collect()
    }

    // Input tests
    #[test]
    fn input_j_moves_down() {
        let mut h = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(h.handle_key(key), Some(Action::MoveDown));
    }

    #[test]
    fn input_k_moves_up() {
        let mut h = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(h.handle_key(key), Some(Action::MoveUp));
    }

    #[test]
    fn input_gg_jumps_top() {
        let mut h = InputHandler::new();
        let g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        assert_eq!(h.handle_key(g), None);
        let g2 = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        assert_eq!(h.handle_key(g2), Some(Action::JumpTop));
    }

    #[test]
    fn input_zz_centers() {
        let mut h = InputHandler::new();
        let z = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE);
        assert_eq!(h.handle_key(z), None);
        let z2 = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE);
        assert_eq!(h.handle_key(z2), Some(Action::CenterCurrent));
    }

    #[test]
    fn input_enter_opens() {
        let mut h = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(h.handle_key(key), Some(Action::OpenSelected));
    }

    #[test]
    fn input_o_opens() {
        let mut h = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE);
        assert_eq!(h.handle_key(key), Some(Action::OpenSelected));
    }

    #[test]
    fn input_escape_back() {
        let mut h = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(h.handle_key(key), Some(Action::Back));
    }

    #[test]
    fn input_q_quits() {
        let mut h = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(h.handle_key(key), Some(Action::QuitView));
    }

    #[test]
    fn input_hml_viewport() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT)),
            Some(Action::ViewportTop)
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('M'), KeyModifiers::SHIFT)),
            Some(Action::ViewportMiddle)
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT)),
            Some(Action::ViewportBottom)
        );
    }

    #[test]
    fn input_ctrl_du_page() {
        let mut h = InputHandler::new();
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            Some(Action::PageDown)
        );
        assert_eq!(
            h.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
            Some(Action::PageUp)
        );
    }

    // App tests
    #[test]
    fn app_move_down() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.apply(Action::MoveDown);
        assert_eq!(app.selected_index, 1);
        app.apply(Action::MoveDown);
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn app_move_up_at_zero() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(5);
        app.apply(Action::MoveUp);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn app_jump_top() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(10);
        app.selected_index = 5;
        app.apply(Action::JumpTop);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn app_switch_pane() {
        let mut app = App::new();
        assert_eq!(app.active_pane, ActivePane::MailList);
        app.apply(Action::SwitchPane);
        assert_eq!(app.active_pane, ActivePane::Sidebar);
        app.apply(Action::SwitchPane);
        assert_eq!(app.active_pane, ActivePane::MailList);
    }

    #[test]
    fn app_quit() {
        let mut app = App::new();
        assert!(!app.should_quit);
        app.apply(Action::QuitView);
        assert!(app.should_quit);
    }

    #[test]
    fn app_move_down_bounds() {
        let mut app = App::new();
        app.envelopes = make_test_envelopes(3);
        app.apply(Action::MoveDown);
        assert_eq!(app.selected_index, 1);
        app.apply(Action::MoveDown);
        assert_eq!(app.selected_index, 2);
        app.apply(Action::MoveDown); // At end
        assert_eq!(app.selected_index, 2);
    }
}
