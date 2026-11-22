use super::*;

#[test]
fn a_delayed_rule_list_preserves_an_unsaved_sorting_preview_and_apply() {
    let mut app = App::new();
    app.rules.page.form.visible = true;
    app.rules.page.form.name = "Owned draft".into();
    app.rules.page.form.condition = "from:owned@example.com".into();
    app.rules.page.form.action = "treatment:reading".into();
    app.rules.action_editor = tui_textarea::TextArea::from(["treatment:reading"]);
    app.rules.condition_editor = tui_textarea::TextArea::from(["from:owned@example.com"]);
    app.apply(Action::ShowRuleDryRun);
    let form = app.rules.pending_sorting_preview.take().unwrap();
    app.apply(Action::ApplyRuleTreatment);
    assert!(
        app.rules.draft_preview_active,
        "Apply before a token must preserve the in-flight draft"
    );
    app.rules.dry_run_request_id = 42;
    let saved = serde_json::json!({"id":"unrelated", "name":"Saved rule"});

    app.replace_rule_list(vec![saved.clone()]);
    assert_eq!(app.rules.dry_run_request_id, 42);
    assert!(app.rules.pending_dry_run.is_none());
    let preview = serde_json::json!({"token":"owned-token", "form":form});
    app.rules.page.dry_run = vec![preview.clone()];
    app.replace_rule_list(vec![saved]);
    assert_eq!(app.rules.page.dry_run, vec![preview]);
    assert_eq!(app.rules.dry_run_request_id, 42);

    app.apply(Action::ApplyRuleTreatment);
    let (applied_form, token) = app.rules.pending_treatment.take().unwrap();
    assert_eq!(
        serde_json::to_value(&applied_form).unwrap(),
        serde_json::to_value(form).unwrap(),
    );
    assert_eq!(token, "owned-token");
    assert!(applied_form.id.is_none());
    assert!(!app.rules.draft_preview_active);
}

#[test]
fn hash_asks_before_deleting_a_rule_and_only_the_confirm_deletes() {
    let mut app = App::new();
    app.screen = Screen::Rules;
    app.rules.page.rules =
        vec![serde_json::json!({"id":"rule-1", "name":"Newsletters", "action":"archive"})];
    let press = |app: &mut App, code: KeyCode| {
        if let Some(action) = app.handle_key(KeyEvent::new(code, KeyModifiers::NONE)) {
            app.apply(action);
        }
    };

    press(&mut app, KeyCode::Char('#'));
    assert!(
        app.rules.pending_delete.is_none(),
        "# deleted without asking"
    );
    let shown = render_to_string(100, 30, |frame| app.draw(frame));
    assert!(shown.contains("Delete rule 'Newsletters'?"), "{shown}");

    press(&mut app, KeyCode::Esc);
    assert!(app.rules.pending_delete.is_none());
    assert!(app.rules.page.delete_confirm.is_none());
    assert_eq!(app.screen, Screen::Rules, "Esc closes the confirm only");

    press(&mut app, KeyCode::Char('#'));
    press(&mut app, KeyCode::Char('n'));
    assert!(app.rules.pending_delete.is_none());

    press(&mut app, KeyCode::Char('#'));
    press(&mut app, KeyCode::Char('y'));
    assert_eq!(app.rules.pending_delete.as_deref(), Some("rule-1"));
    assert!(app.rules.page.delete_confirm.is_none());
}
