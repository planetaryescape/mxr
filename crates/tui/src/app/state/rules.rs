#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesPanel {
    Details,
    History,
    DryRun,
    Form,
}

#[derive(Debug, Clone, Default)]
pub struct RuleFormState {
    pub visible: bool,
    pub existing_rule: Option<String>,
    pub account_id: Option<mxr_core::AccountId>,
    pub name: String,
    pub condition: String,
    pub action: String,
    pub priority: String,
    pub enabled: bool,
    pub active_field: usize,
    /// Phase 2.4: surfaced to the user when the rule form rejects a
    /// submit (empty `shell:` command, blank action, etc). Cleared
    /// on the next successful interaction.
    pub validation_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RulesPageState {
    pub rules: Vec<serde_json::Value>,
    pub selected_index: usize,
    pub detail: Option<serde_json::Value>,
    pub history: Vec<serde_json::Value>,
    pub dry_run: Vec<serde_json::Value>,
    pub panel: RulesPanel,
    pub status: Option<String>,
    pub refresh_pending: bool,
    pub form: RuleFormState,
}

impl Default for RulesPageState {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            selected_index: 0,
            detail: None,
            history: Vec::new(),
            dry_run: Vec::new(),
            panel: RulesPanel::Details,
            status: None,
            refresh_pending: false,
            form: RuleFormState {
                enabled: true,
                priority: "100".to_string(),
                ..RuleFormState::default()
            },
        }
    }
}

#[derive(Default)]
pub struct RulesState {
    pub draft_preview_active: bool,
    pub page: RulesPageState,
    pub pending_detail: Option<String>,
    pub detail_request_id: u64,
    pub pending_history: Option<String>,
    pub history_request_id: u64,
    pub pending_dry_run: Option<String>,
    pub dry_run_request_id: u64,
    pub pending_sorting_preview: Option<mxr_protocol::RuleFormData>,
    pub pending_treatment: Option<(mxr_protocol::RuleFormData, String)>,
    pub pending_delete: Option<String>,
    pub pending_upsert: Option<serde_json::Value>,
    pub pending_form_load: Option<String>,
    pub form_request_id: u64,
    pub pending_form_save: bool,
    pub condition_editor: TextArea<'static>,
    pub action_editor: TextArea<'static>,
}
use tui_textarea::TextArea;
