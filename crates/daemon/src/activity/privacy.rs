//! Startup policy shared by mapped requests, direct producers and forced markers.
//! Keep these keys aligned with the activity producers, not arbitrary mail JSON.

use mxr_config::ActivityConfig;
use serde_json::Value;

use super::OwnedEntry;

pub(super) fn prepare(config: &ActivityConfig, entry: &mut OwnedEntry) -> bool {
    if !config.track_link_clicks && entry.action == "link.click" {
        return false;
    }
    // Sender targets are email addresses, unlike opaque message/account IDs.
    if !config.track_recipient_handles && entry.target_kind.as_deref() == Some("sender") {
        entry.target_id = None;
    }
    if let Some(context) = &mut entry.context {
        redact_context(config, context);
    }
    true
}

fn redact_context(config: &ActivityConfig, value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.retain(|key, value| {
                let omit = match key.as_str() {
                    "subject" => !config.track_subjects,
                    "query" => !config.track_search_queries,
                    "email" | "sender_email" | "recipient_handles" | "recipients" => {
                        !config.track_recipient_handles
                    }
                    "url" => !config.track_link_clicks,
                    // `to` is a folder destination and `name` can be a rule,
                    // label or saved-search name. Neither is a recipient key.
                    _ => false,
                };
                if !omit {
                    redact_context(config, value);
                }
                !omit
            });
        }
        Value::Array(items) => {
            for item in items {
                redact_context(config, item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_privacy_removes_fields_regardless_of_value_type_or_container_name() {
        let config = ActivityConfig {
            track_subjects: false,
            track_search_queries: false,
            track_recipient_handles: false,
            ..Default::default()
        };
        let mut context = serde_json::json!({
            "__proto__": {"subject": {"nested": "private"}, "name": "kept"},
            "items": [null, 42, "kept", {"query": ["private"], "email": null, "sender_email": false, "url": 1}],
            "to": "Folder", "name": "Rule", "to_count": 2
        });
        redact_context(&config, &mut context);
        assert_eq!(
            context,
            serde_json::json!({
                "__proto__": {"name": "kept"}, "items": [null, 42, "kept", {}],
                "to": "Folder", "name": "Rule", "to_count": 2
            })
        );
    }
}
