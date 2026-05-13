use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SafetyConfig {
    #[serde(default)]
    pub recipients: SafetyRecipientConfig,
    #[serde(default)]
    pub tone: SafetyToneConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SafetyRecipientConfig {
    #[serde(default)]
    pub internal_domains: Vec<String>,
    #[serde(default)]
    pub sensitive_domains: Vec<String>,
    #[serde(default)]
    pub warn_on_first_time_external: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyToneConfig {
    /// Formality-score delta threshold above which a tone warning fires.
    /// Defaults to 0.4, which corresponds to roughly half the formality
    /// scale.
    pub formality_delta_threshold: f64,
}

impl Default for SafetyToneConfig {
    fn default() -> Self {
        Self {
            formality_delta_threshold: 0.4,
        }
    }
}
