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
    pub name: String,
    pub condition: String,
    pub action: String,
    pub priority: String,
    pub enabled: bool,
    pub active_field: usize,
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
    pub page: RulesPageState,
    pub pending_detail: Option<String>,
    pub detail_request_id: u64,
    pub pending_history: Option<String>,
    pub history_request_id: u64,
    pub pending_dry_run: Option<String>,
    pub pending_delete: Option<String>,
    pub pending_upsert: Option<serde_json::Value>,
    pub pending_form_load: Option<String>,
    pub form_request_id: u64,
    pub pending_form_save: bool,
    pub condition_editor: TextArea<'static>,
    pub action_editor: TextArea<'static>,
}
use tui_textarea::TextArea;
