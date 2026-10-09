use super::*;

impl App {
    pub fn replace_rule_list(&mut self, rules: Vec<serde_json::Value>) {
        self.rules.page.rules = rules;
        self.rules.page.selected_index = self
            .rules
            .page
            .selected_index
            .min(self.rules.page.rules.len().saturating_sub(1));
        // The list can arrive after a draft preview starts; it does not own that preview.
        if !self.rules.draft_preview_active {
            self.refresh_selected_rule_panel();
        }
    }

    pub(super) fn apply_rule_action(&mut self, action: Action) {
        match action {
            Action::RefreshRules => {
                self.rules.page.refresh_pending = true;
                self.refresh_selected_rule_panel();
            }
            Action::ToggleRuleEnabled => {
                if let Some(rule) = self.selected_rule().cloned() {
                    let mut updated = rule.clone();
                    if let Some(enabled) =
                        updated.get("enabled").and_then(serde_json::Value::as_bool)
                    {
                        updated["enabled"] = serde_json::Value::Bool(!enabled);
                        self.rules.pending_upsert = Some(updated);
                        self.rules.page.status = Some(if enabled {
                            "Disabling rule...".into()
                        } else {
                            "Enabling rule...".into()
                        });
                    }
                }
            }
            Action::DeleteRule => {
                if let Some(rule_id) = self
                    .selected_rule()
                    .and_then(|rule| rule["id"].as_str())
                    .map(ToString::to_string)
                {
                    self.rules.pending_delete = Some(rule_id.clone());
                    self.rules.page.status = Some(format!("Deleting {rule_id}..."));
                }
            }
            Action::ShowRuleHistory => {
                self.rules.page.panel = RulesPanel::History;
                self.refresh_selected_rule_panel();
            }
            Action::ApplyRuleTreatment => {
                self.rules.draft_preview_active = false;
                if let Some(preview) = self.rules.page.dry_run.first() {
                    if let (Some(token), Ok(form)) = (
                        preview["token"].as_str(),
                        serde_json::from_value::<mxr_protocol::RuleFormData>(
                            preview["form"].clone(),
                        ),
                    ) {
                        self.rules.pending_treatment = Some((form, token.to_string()));
                        self.rules.page.dry_run.clear();
                        self.rules.page.status = Some("Applying the sorting preview...".into());
                    }
                }
            }
            Action::ShowRuleDryRun => {
                if self.rules.page.form.visible
                    && self.rules.page.form.action.contains("treatment:")
                {
                    self.rules.draft_preview_active = true;
                    self.rules.pending_dry_run = None;
                    self.sync_rule_form_strings_from_editors();
                    let f = &self.rules.page.form;
                    self.rules.pending_sorting_preview = Some(mxr_protocol::RuleFormData {
                        id: f.existing_rule.clone(),
                        account_id: f.account_id.clone(),
                        name: f.name.clone(),
                        condition: f.condition.clone(),
                        action: f.action.clone(),
                        priority: f.priority.parse().unwrap_or(100),
                        enabled: f.enabled,
                    });
                    self.rules.page.form.visible = false;
                    self.rules.page.panel = RulesPanel::DryRun;
                    self.rules.page.dry_run.clear();
                    return;
                }
                self.rules.page.panel = RulesPanel::DryRun;
                self.refresh_selected_rule_panel();
            }
            Action::OpenRuleFormNew => {
                self.rules.page.form = RuleFormState {
                    visible: true,
                    account_id: self.default_account_id().cloned(),
                    enabled: true,
                    priority: "100".to_string(),
                    active_field: 0,
                    ..RuleFormState::default()
                };
                self.sync_rule_form_editors();
                self.rules.page.panel = RulesPanel::Form;
            }
            Action::OpenRuleFormEdit => {
                if let Some(rule_id) = self
                    .selected_rule()
                    .and_then(|rule| rule["id"].as_str())
                    .map(ToString::to_string)
                {
                    self.rules.pending_form_load = Some(rule_id);
                }
            }
            Action::SaveRuleForm => {
                self.sync_rule_form_strings_from_editors();
                if let Some(error) = rule_form_validation_error(&self.rules.page.form) {
                    self.rules.page.form.validation_error = Some(error);
                    return;
                }
                self.rules.page.form.validation_error = None;
                self.rules.page.status = Some("Saving rule...".into());
                self.rules.pending_form_save = true;
            }
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}

/// Phase 2.4: client-side validation for rule form submits. Catches
/// the cases the daemon would either silently accept (e.g. empty
/// `shell:` command yielding a no-op rule) or reject far from the
/// UX with a generic "Unsupported action" string.
pub(crate) fn rule_form_validation_error(form: &RuleFormState) -> Option<String> {
    if form.name.trim().is_empty() {
        return Some("Rule name is required".into());
    }
    if form.condition.trim().is_empty() {
        return Some("Condition is required".into());
    }
    let action = form.action.trim();
    if action.is_empty() {
        return Some("Action is required (e.g. mark-read,archive or add-label:GitHub)".into());
    }
    if let Some(command) = action.strip_prefix("shell:") {
        if command.trim().is_empty() {
            return Some("Shell-hook command is required after `shell:`".into());
        }
    }
    None
}
