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

#[test]
fn category_rule_preview_scroll_reaches_final_effect_at_80_columns_without_mutation() {
    let mut app = App::new();
    app.screen = Screen::Rules;
    app.rules.page.rules = vec![serde_json::json!({"id":"selected", "name":"Flight mail"})];
    app.rules.page.panel = crate::app::RulesPanel::DryRun;
    let form = mxr_protocol::RuleFormData {
        id: Some("selected".into()),
        account_id: None,
        name: "Flight mail".into(),
        condition: "subject:flight".into(),
        action: "add-category:Travel".into(),
        priority: 100,
        enabled: true,
    };
    let mut matches: Vec<_> = (0..12)
        .map(|i| {
            serde_json::json!({
                "category_name":"Travel", "subject":format!("Flight effect {i}"),
                "before":false, "after":true,
                "reason":"Winning rule Flight mail; manual decisions remain unchanged"
            })
        })
        .collect();
    matches[0]["subject"] =
        serde_json::json!(format!("Flight effect 0 {}", "long subject ".repeat(1000)));
    app.rules.page.dry_run = vec![serde_json::json!({
        "category":true, "token":"scroll-token", "form":form,
        "result":{"complete":true, "matches":matches}
    })];
    let original = app.rules.page.dry_run.clone();
    let first = render_to_string(80, 24, |frame| app.draw(frame));
    assert!(first.contains("PgUp/PgDn:effects"), "{first}");
    assert!(first.contains("Travel: Unassigned → Assigned"), "{first}");
    assert!(first.contains("Winning rule Flight mail"), "{first}");
    assert!(first.contains("Subject: Flight effect 0"), "{first}");
    assert!(
        first.contains("..."),
        "long subject must use a bounded display"
    );
    assert!(
        !first.contains("Flight effect 11"),
        "fixture must exceed the viewport"
    );
    let mut last = first.clone();
    for _ in 0..20 {
        assert!(app
            .handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
            .is_none());
        last = render_to_string(80, 24, |frame| app.draw(frame));
    }
    assert!(last.contains("Flight effect 11"), "{last}");
    assert_eq!(app.rules.page.dry_run_page, 11);
    assert!(last.contains("Dry Run 12/12"));
    assert_eq!(app.rules.page.selected_index, 0);
    assert_eq!(app.rules.page.dry_run, original);
    assert!(app.rules.pending_category_preview.is_none());
    assert!(app.rules.pending_category_apply.is_none());
    for _ in 0..20 {
        app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
    }
    assert_eq!(render_to_string(80, 24, |frame| app.draw(frame)), first);
    app.rules.page.dry_run_page = 5;
    app.apply(Action::ShowRuleDryRun);
    assert_eq!(app.rules.page.dry_run_page, 0);
}

#[test]
fn category_rule_form_can_preview_all_accounts_and_switch_back() {
    let mut app = App::new();
    let account_id = AccountId::new();
    app.accounts.page.accounts = vec![account_summary(account_id.clone(), true, true)];
    app.screen = Screen::Rules;
    app.apply(Action::OpenRuleFormNew);
    assert_eq!(app.rules.page.form.account_id, Some(account_id.clone()));
    app.rules.page.form.name = "Flight mail".into();
    app.rules.page.form.condition = "subject:flight".into();
    app.rules.page.form.action = "add-category:Travel".into();
    app.rules.page.form.active_field = 5;
    app.sync_rule_form_editors();
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert!(app.rules.page.form.account_id.is_none());
    let global = render_to_string(80, 24, |frame| app.draw(frame));
    assert!(global.contains("All accounts"), "{global}");
    app.apply(Action::ShowRuleDryRun);
    assert!(app
        .rules
        .pending_category_preview
        .take()
        .unwrap()
        .0
        .account_id
        .is_none());
    app.rules.page.form.visible = true;
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(app.rules.page.form.account_id, Some(account_id.clone()));
    app.apply(Action::ShowRuleDryRun);
    assert_eq!(
        app.rules
            .pending_category_preview
            .take()
            .unwrap()
            .0
            .account_id,
        Some(account_id.clone())
    );

    // Treatment-only forms retain their existing account-choice policy.
    app.rules.page.form.visible = true;
    app.rules.page.form.action = "treatment:reading".into();
    app.sync_rule_form_editors();
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(app.rules.page.form.account_id, Some(account_id));
}

#[test]
fn category_rule_preview_pages_beyond_65k_effects() {
    let mut app = App::new();
    app.screen = Screen::Rules;
    app.rules.page.panel = crate::app::RulesPanel::DryRun;
    let mut matches =
        vec![serde_json::json!({"subject":"Flight", "reason":"Winning rule"}); 70_000];
    matches[69_999]["subject"] = serde_json::json!("Final flight effect");
    app.rules.page.dry_run = vec![serde_json::json!({
        "token":"large-token", "result":{"complete":true, "matches":matches}
    })];
    app.rules.page.dry_run_page = 65_535;
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    assert_eq!(app.rules.page.dry_run_page, 65_536);
    app.rules.page.dry_run_page = 69_998;
    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    let final_page = render_to_string(80, 24, |frame| app.draw(frame));
    assert!(final_page.contains("Final flight effect"), "{final_page}");
    assert!(final_page.contains("Dry Run 70000/70000"), "{final_page}");
    assert_eq!(app.rules.page.dry_run[0]["token"], "large-token");
    assert!(app.rules.pending_category_apply.is_none());
}
