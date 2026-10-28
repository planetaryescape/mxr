use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mxr_core::{parse_account_tag, AccountColor};

impl App {
    /// `i` on the Accounts list: edit the selected account's colour and tag
    /// (blueprint 33). Only configured accounts have an identity to write.
    pub(super) fn open_identity_editor(&mut self) {
        let Some(account) = self.selected_account().cloned() else {
            return;
        };
        let Some(key) = account.key.clone() else {
            self.accounts.page.status =
                Some("Runtime-only account: its colour and tag cannot be set here.".into());
            return;
        };
        let Some(identity) = account.identity.clone() else {
            self.accounts.page.status = Some(
                "This daemon predates account identity; upgrade it to set a colour and tag.".into(),
            );
            return;
        };
        self.accounts.page.identity_editor = Some(AccountIdentityEditor {
            account_key: key,
            account_name: account.name,
            color: identity.color,
            tag: identity.tag,
            field: IdentityField::Colour,
            previewed: false,
            in_flight_dry_run: None,
            message: None,
        });
    }

    pub(super) fn handle_identity_editor_key(&mut self, key: KeyEvent) -> Option<Action> {
        let editing = self
            .accounts
            .page
            .identity_editor
            .as_ref()
            .is_some_and(|editor| editor.in_flight_dry_run.is_none());
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.accounts.page.identity_editor = None;
            }
            _ if !editing => {}
            (KeyCode::Enter, _) => self.submit_identity_editor(),
            (KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down, _) => {
                if let Some(editor) = self.accounts.page.identity_editor.as_mut() {
                    editor.field = match editor.field {
                        IdentityField::Colour => IdentityField::Tag,
                        IdentityField::Tag => IdentityField::Colour,
                    };
                    editor.changed();
                }
            }
            (KeyCode::Left | KeyCode::Right | KeyCode::Char(' '), _) => {
                let forward = key.code != KeyCode::Left;
                if let Some(editor) = self.accounts.page.identity_editor.as_mut() {
                    if editor.field == IdentityField::Colour {
                        editor.color = cycle_colour(editor.color, forward);
                        editor.changed();
                    }
                }
            }
            (KeyCode::Backspace, _) => {
                if let Some(editor) = self.accounts.page.identity_editor.as_mut() {
                    if editor.field == IdentityField::Tag {
                        editor.tag.pop();
                        editor.changed();
                    }
                }
            }
            (KeyCode::Char(c), modifiers)
                if (modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT)
                    && c.is_ascii_alphanumeric() =>
            {
                if let Some(editor) = self.accounts.page.identity_editor.as_mut() {
                    if editor.field == IdentityField::Tag && editor.tag.len() < 3 {
                        editor.tag.push(c.to_ascii_uppercase());
                        editor.changed();
                    }
                }
            }
            _ => {}
        }
        None
    }

    /// The first Enter previews the change through the daemon's dry run; the
    /// next Enter, with nothing edited since, saves it.
    fn submit_identity_editor(&mut self) {
        let Some(editor) = self.accounts.page.identity_editor.as_mut() else {
            return;
        };
        let tag = match parse_account_tag(&editor.tag) {
            Ok(tag) => tag,
            Err(error) => {
                editor.message = Some(format!("{error}"));
                return;
            }
        };
        let dry_run = !editor.previewed;
        editor.in_flight_dry_run = Some(dry_run);
        editor.message = None;
        let request = (editor.account_key.clone(), editor.color, tag, dry_run);
        self.accounts.page.operation_in_flight = true;
        self.accounts.pending_identity = Some(request);
    }

    /// Called with the daemon's answer to an identity write, or with `None`
    /// when the request failed. A saved change closes the editor; a dry run
    /// that passes leaves it open for the second Enter.
    pub(crate) fn finish_identity_write(
        &mut self,
        result: Option<&mxr_protocol::AccountOperationResult>,
    ) {
        let Some(editor) = self.accounts.page.identity_editor.as_mut() else {
            return;
        };
        let Some(dry_run) = editor.in_flight_dry_run.take() else {
            return;
        };
        match result {
            Some(result) if result.ok && !dry_run => {
                self.accounts.page.identity_editor = None;
            }
            Some(result) if result.ok => {
                editor.previewed = true;
                editor.message = Some(step_detail(result));
            }
            Some(result) => {
                editor.previewed = false;
                editor.message = Some(step_detail(result));
            }
            None => {
                editor.previewed = false;
                editor.message = Some("The daemon did not answer; nothing was saved.".into());
            }
        }
    }
}

fn step_detail(result: &mxr_protocol::AccountOperationResult) -> String {
    [&result.save, &result.auth, &result.sync, &result.send]
        .into_iter()
        .flatten()
        .map(|step| step.detail.clone())
        .find(|detail| !detail.is_empty())
        .unwrap_or_else(|| result.summary.clone())
}

fn cycle_colour(current: AccountColor, forward: bool) -> AccountColor {
    let slots = AccountColor::ALL;
    let index = slots
        .iter()
        .position(|color| *color == current)
        .unwrap_or_default();
    let next = if forward {
        (index + 1) % slots.len()
    } else {
        (index + slots.len() - 1) % slots.len()
    };
    slots[next]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    fn press(app: &mut App, code: KeyCode) {
        app.handle_identity_editor_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn editor(color: AccountColor, tag: &str) -> AccountIdentityEditor {
        AccountIdentityEditor {
            account_key: "work".into(),
            account_name: "Work".into(),
            color,
            tag: tag.into(),
            field: IdentityField::Colour,
            previewed: false,
            in_flight_dry_run: None,
            message: None,
        }
    }

    #[test]
    fn first_enter_previews_and_second_enter_saves_unless_edited() {
        let mut app = App::new();
        app.accounts.page.identity_editor = Some(editor(AccountColor::Sky, "W"));

        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.accounts.pending_identity,
            Some(("work".into(), AccountColor::Sky, "W".into(), true)),
            "first Enter is a dry run"
        );
        app.accounts.pending_identity = None;
        app.finish_identity_write(Some(&mxr_protocol::AccountOperationResult {
            ok: true,
            summary: "Would set identity".into(),
            save: Some(mxr_protocol::AccountOperationStep {
                ok: true,
                detail: "Would set identity for account 'work' (sky, W).".into(),
            }),
            auth: None,
            sync: None,
            send: None,
            device_code_url: None,
            device_code_user_code: None,
        }));
        let open = app
            .accounts
            .page
            .identity_editor
            .clone()
            .expect("editor stays open");
        assert!(open.previewed);

        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.accounts.pending_identity,
            Some(("work".into(), AccountColor::Sky, "W".into(), false)),
            "second Enter saves"
        );

        // Keys are ignored while the save is in flight.
        press(&mut app, KeyCode::Right);
        assert_eq!(
            app.accounts.page.identity_editor.as_ref().unwrap().color,
            AccountColor::Sky,
            "keys wait for the daemon's answer"
        );
    }

    #[test]
    fn colour_cycles_through_the_eight_slots_and_tag_keeps_three_characters() {
        let mut app = App::new();
        app.accounts.page.identity_editor = Some(editor(AccountColor::Slate, ""));

        press(&mut app, KeyCode::Right);
        assert_eq!(
            app.accounts.page.identity_editor.as_ref().unwrap().color,
            AccountColor::Sky,
            "slate wraps to sky"
        );

        press(&mut app, KeyCode::Tab);
        for c in ['p', 'e', 'x', 'y'] {
            press(&mut app, KeyCode::Char(c));
        }
        assert_eq!(
            app.accounts.page.identity_editor.as_ref().unwrap().tag,
            "PEX",
            "a tag holds three characters, uppercased"
        );
    }

    #[test]
    fn a_refused_dry_run_keeps_the_editor_open_with_the_daemon_words() {
        let mut app = App::new();
        app.accounts.page.identity_editor = Some(editor(AccountColor::Sky, "W"));
        press(&mut app, KeyCode::Enter);
        app.finish_identity_write(Some(&mxr_protocol::AccountOperationResult {
            ok: false,
            summary: "Failed to set identity".into(),
            save: Some(mxr_protocol::AccountOperationStep {
                ok: false,
                detail: "tag 'W' is already held by account 'home'".into(),
            }),
            auth: None,
            sync: None,
            send: None,
            device_code_url: None,
            device_code_user_code: None,
        }));
        let open = app
            .accounts
            .page
            .identity_editor
            .clone()
            .expect("editor stays open");
        assert_eq!(
            open.message.as_deref(),
            Some("tag 'W' is already held by account 'home'")
        );
        assert!(!open.previewed);
    }

    #[test]
    fn an_edit_after_a_passed_dry_run_makes_the_next_enter_preview_again() {
        let mut app = App::new();
        let mut previewed = editor(AccountColor::Sky, "W");
        previewed.previewed = true;
        app.accounts.page.identity_editor = Some(previewed);

        press(&mut app, KeyCode::Right);
        let after = app
            .accounts
            .page
            .identity_editor
            .clone()
            .expect("editor open");
        assert!(!after.previewed);
        assert_eq!(after.color, AccountColor::Amber);
    }

    /// With single-key shortcuts off, the tag must still take letters.
    #[test]
    fn the_identity_tag_counts_as_text_input_and_the_colour_does_not() {
        let mut app = App::new();
        app.accounts.page.identity_editor = Some(editor(AccountColor::Sky, "W"));
        assert!(
            !app.text_input_focused(),
            "the colour field takes no letters"
        );
        app.accounts.page.identity_editor.as_mut().unwrap().field = IdentityField::Tag;
        assert!(app.text_input_focused());
    }
}
