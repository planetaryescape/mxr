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

    // First while the draft request is in flight, then after its token arrives.
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
