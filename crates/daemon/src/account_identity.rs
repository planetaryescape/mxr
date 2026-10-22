//! Resolves each account's colour and tag for the mail marks (blueprint 33).
//!
//! The store keeps each account's colour and tag from the moment it is
//! created. Config can override either one. Config is read as written, so
//! this module decides what to use: a value that is not valid, or that clashes,
//! is ignored with a warning, and the stored value is used.

use crate::state::AppState;
use mxr_core::{
    derive_account_tag, next_account_color, parse_account_tag, AccountColor, AccountId,
    NoFreeAccountTag,
};
use mxr_protocol::AccountIdentityData;
use std::collections::{HashMap, HashSet};

/// One account in creation order. The config fields are the raw strings
/// from the file, which may not be valid.
#[derive(Debug, Clone)]
pub struct IdentityInput {
    pub account_id: AccountId,
    pub name: String,
    /// The config key, when config defines the account.
    pub key: Option<String>,
    pub stored_color: Option<AccountColor>,
    pub stored_tag: Option<String>,
    pub config_color: Option<String>,
    pub config_tag: Option<String>,
}

/// One account's resolved identity.
#[derive(Debug, Clone)]
pub struct ResolvedIdentity {
    pub account_id: AccountId,
    pub name: String,
    pub identity: AccountIdentityData,
    /// The tag the store holds for this account, or the one it will be given
    /// when its row is written. Other accounts may not take it.
    pub stored_tag: String,
}

/// An override that was ignored, and why. `key` is the config key, or the
/// account name when config does not define the account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityWarning {
    pub key: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Resolution {
    /// Same order as the input.
    pub resolved: Vec<ResolvedIdentity>,
    pub warnings: Vec<IdentityWarning>,
}

/// Resolves every account's colour and tag.
///
/// Colour is the config override, else the stored colour. Tag is the config
/// override, else the stored tag. A stored tag is never derived again, so a
/// rename, a removal or an override on one account cannot change another's.
///
/// An override is ignored, with a warning, when it is not valid (a colour
/// that is not a slot name, a tag that is not 1 to 3 letters or digits), when
/// another account stores that tag, or when an earlier account configures it.
///
/// An account with no stored value gets the one insertion will give it:
/// `next_account_color` and a derived tag, both against what is stored and
/// the configured tags, counting accounts in input order.
pub fn resolve_identities(inputs: &[IdentityInput]) -> Result<Resolution, NoFreeAccountTag> {
    let label = |index: usize| {
        inputs[index]
            .key
            .clone()
            .unwrap_or_else(|| inputs[index].name.clone())
    };
    let mut warnings = Vec::new();

    let mut color_overrides = Vec::with_capacity(inputs.len());
    let mut tag_overrides: Vec<Option<String>> = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        color_overrides.push(match &input.config_color {
            None => None,
            Some(raw) => match raw.parse::<AccountColor>() {
                Ok(color) => Some(color),
                Err(error) => {
                    warnings.push(IdentityWarning {
                        key: label(index),
                        message: format!("{error}; using the stored colour"),
                    });
                    None
                }
            },
        });
        tag_overrides.push(match &input.config_tag {
            None => None,
            Some(raw) => match parse_account_tag(raw) {
                Ok(tag) => Some(tag),
                Err(error) => {
                    warnings.push(IdentityWarning {
                        key: label(index),
                        message: format!("{error}; using the stored tag"),
                    });
                    None
                }
            },
        });
    }

    let reserved: HashSet<String> = tag_overrides.iter().flatten().cloned().collect();
    let mut taken_colors: Vec<AccountColor> = inputs
        .iter()
        .filter_map(|input| input.stored_color)
        .collect();
    let mut taken_tags: HashSet<String> = inputs
        .iter()
        .filter_map(|input| input.stored_tag.clone())
        .chain(reserved)
        .collect();
    let holders: HashMap<&str, usize> = inputs
        .iter()
        .enumerate()
        .filter_map(|(index, input)| Some((input.stored_tag.as_deref()?, index)))
        .collect();

    let mut claimed: HashMap<String, usize> = HashMap::new();
    let mut resolved = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        let stored_color = match input.stored_color {
            Some(color) => color,
            None => {
                let color = next_account_color(&taken_colors, index);
                taken_colors.push(color);
                color
            }
        };

        let stored_tag = match &input.stored_tag {
            Some(tag) => tag.clone(),
            None => {
                let tag = derive_account_tag(&input.name, &taken_tags, &input.account_id.as_str())?;
                taken_tags.insert(tag.clone());
                tag
            }
        };

        let config_tag = match tag_overrides[index].clone() {
            None => None,
            Some(tag) => match holders.get(tag.as_str()) {
                Some(&holder) if holder != index => {
                    warnings.push(IdentityWarning {
                        key: label(index),
                        message: format!(
                            "tag '{tag}' is already held by account '{}'; using the stored tag",
                            label(holder)
                        ),
                    });
                    None
                }
                _ => match claimed.get(&tag) {
                    Some(&first) => {
                        warnings.push(IdentityWarning {
                            key: label(index),
                            message: format!(
                                "tag '{tag}' is already set on account '{}'; using the stored tag",
                                label(first)
                            ),
                        });
                        None
                    }
                    None => {
                        claimed.insert(tag.clone(), index);
                        Some(tag)
                    }
                },
            },
        };

        resolved.push(ResolvedIdentity {
            account_id: input.account_id.clone(),
            name: input.name.clone(),
            identity: AccountIdentityData {
                color: color_overrides[index].unwrap_or(stored_color),
                tag: config_tag.unwrap_or_else(|| stored_tag.clone()),
            },
            stored_tag,
        });
    }
    Ok(Resolution { resolved, warnings })
}

/// The colour override from config, when it is a valid slot name.
pub fn config_color(account: &mxr_config::AccountConfig) -> Option<AccountColor> {
    account.color.as_deref().and_then(|raw| raw.parse().ok())
}

/// The name of the account that already has `tag`: one that shows it or
/// stores it, or one that config defines with it. `configured` is each
/// account's name and valid tag, for accounts config defines.
pub fn tag_holder<'a>(
    tag: &str,
    others: &'a [ResolvedIdentity],
    configured: &'a [(String, String)],
) -> Option<&'a str> {
    others
        .iter()
        .find(|other| other.identity.tag == tag || other.stored_tag == tag)
        .map(|other| other.name.as_str())
        .or_else(|| {
            configured
                .iter()
                .find(|(_, configured_tag)| configured_tag == tag)
                .map(|(name, _)| name.as_str())
        })
}

/// The refusal for a tag another account holds. A caller whose allowlist does
/// not cover that account is not told its name, since the name could leak it.
pub fn clash_message(tag: &str, holder: &str, reveal_holder: bool) -> String {
    if reveal_holder {
        format!("Tag '{tag}' is already used by account '{holder}'.")
    } else {
        format!("Tag '{tag}' is already used by another account.")
    }
}

/// The name and valid tag of every account config defines, except `exclude_key`.
pub fn configured_tags(config: &mxr_config::MxrConfig, exclude_key: &str) -> Vec<(String, String)> {
    config
        .accounts
        .iter()
        .filter(|(key, _)| key.as_str() != exclude_key)
        .filter_map(|(_, account)| {
            let tag = parse_account_tag(account.tag.as_deref()?).ok()?;
            Some((account.name.clone(), tag))
        })
        .collect()
}

/// Checks a requested colour and tag for one account and returns the identity
/// before and after. A dry run and the real write both call this, so they agree.
/// `others` are the other accounts and `configured` the other configured tags.
/// `reveal_holder` is false for a scoped caller.
pub fn plan_identity_change(
    current: &ResolvedIdentity,
    color: Option<AccountColor>,
    tag: Option<&str>,
    others: &[ResolvedIdentity],
    configured: &[(String, String)],
    reveal_holder: bool,
) -> Result<(AccountIdentityData, AccountIdentityData), String> {
    if let Some(tag) = tag {
        if let Some(holder) = tag_holder(tag, others, configured) {
            return Err(clash_message(tag, holder, reveal_holder));
        }
    }
    let after = AccountIdentityData {
        color: color.unwrap_or(current.identity.color),
        tag: tag.map_or_else(|| current.identity.tag.clone(), str::to_string),
    };
    Ok((current.identity.clone(), after))
}

/// Writes a colour and tag override into `config` for one account. It touches
/// nothing else: `default_account` and the account's other fields are left as
/// they were, including any values the daemon ignores.
pub fn set_identity_in_config(
    config: &mut mxr_config::MxrConfig,
    key: &str,
    color: Option<AccountColor>,
    tag: Option<&str>,
) -> Result<(), String> {
    let account = config
        .accounts
        .get_mut(key)
        .ok_or_else(|| format!("Account '{key}' not found"))?;
    if let Some(color) = color {
        account.color = Some(color.as_str().to_string());
    }
    if let Some(tag) = tag {
        account.tag = Some(parse_account_tag(tag).map_err(|e| e.to_string())?);
    }
    Ok(())
}

/// Gathers the stored values and config for every account and resolves them.
/// Config accounts with no stored row follow the stored ones, sorted by key.
pub async fn resolve_from_store(
    store: &mxr_store::Store,
    config: &mxr_config::MxrConfig,
) -> Result<Resolution, String> {
    let slots = store
        .list_account_identity_slots()
        .await
        .map_err(|error| error.to_string())?;

    let mut inputs = Vec::new();
    let mut seen = HashSet::new();
    for slot in slots {
        let configured = config.accounts.iter().find(|(key, account)| {
            crate::handler::config_account_id(key, account) == slot.account_id
        });
        let (name, key, config_color, config_tag) = match configured {
            Some((key, account)) => (
                account.name.clone(),
                Some(key.clone()),
                account.color.clone(),
                account.tag.clone(),
            ),
            None => (slot.name.clone(), None, None, None),
        };
        seen.insert(slot.account_id.clone());
        inputs.push(IdentityInput {
            account_id: slot.account_id,
            name,
            key,
            stored_color: slot.color,
            stored_tag: slot.tag,
            config_color,
            config_tag,
        });
    }

    let mut unstored: Vec<_> = config
        .accounts
        .iter()
        .filter(|(key, account)| !seen.contains(&crate::handler::config_account_id(key, account)))
        .collect();
    unstored.sort_by_key(|(key, _)| *key);
    for (key, account) in unstored {
        inputs.push(IdentityInput {
            account_id: crate::handler::config_account_id(key, account),
            name: account.name.clone(),
            key: Some(key.clone()),
            stored_color: None,
            stored_tag: None,
            config_color: account.color.clone(),
            config_tag: account.tag.clone(),
        });
    }

    resolve_identities(&inputs).map_err(|error| error.to_string())
}

pub async fn resolve_for_state(state: &AppState) -> Result<Resolution, String> {
    resolve_from_store(&state.store, &state.config_snapshot()).await
}

/// Logs each ignored override. Called where config is loaded or reloaded, so
/// the operator sees why a configured value is not in effect.
pub fn log_warnings(warnings: &[IdentityWarning]) {
    for warning in warnings {
        tracing::warn!(
            account = %warning.key,
            "ignoring this account's colour or tag override: {}",
            warning.message
        );
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;

    fn input(name: &str, stored_tag: Option<&str>) -> IdentityInput {
        IdentityInput {
            account_id: AccountId::new(),
            name: name.to_string(),
            key: None,
            stored_color: None,
            stored_tag: stored_tag.map(str::to_string),
            config_color: None,
            config_tag: None,
        }
    }

    fn resolved(name: &str, tag: &str, stored_tag: &str) -> ResolvedIdentity {
        ResolvedIdentity {
            account_id: AccountId::new(),
            name: name.into(),
            identity: AccountIdentityData {
                color: AccountColor::Sky,
                tag: tag.into(),
            },
            stored_tag: stored_tag.into(),
        }
    }

    #[test]
    fn stored_tag_is_used_even_after_a_rename() {
        let mut office = input("Office", Some("W"));
        office.stored_color = Some(AccountColor::Sky);
        let resolution = resolve_identities(&[office]).unwrap();
        assert_eq!(resolution.resolved[0].identity.tag, "W");
        assert_eq!(resolution.resolved[0].stored_tag, "W");
    }

    #[test]
    fn config_tag_wins_over_the_stored_tag() {
        let mut work = input("Work", Some("W"));
        work.config_tag = Some("OFF".into());
        let resolution = resolve_identities(&[work]).unwrap();
        assert_eq!(resolution.resolved[0].identity.tag, "OFF");
        assert_eq!(resolution.resolved[0].stored_tag, "W");
        assert!(resolution.warnings.is_empty());
    }

    #[test]
    fn configured_tag_held_by_another_stored_account_is_ignored_with_a_warning() {
        let mut work = input("Work", Some("W"));
        work.key = Some("work".into());
        let mut wedding = input("Wedding", Some("WE"));
        wedding.key = Some("wedding".into());
        wedding.config_tag = Some("W".into());
        let resolution = resolve_identities(&[work, wedding]).unwrap();
        assert_eq!(resolution.resolved[1].identity.tag, "WE");
        assert_eq!(resolution.resolved[0].identity.tag, "W");
        assert_eq!(
            resolution.warnings,
            vec![IdentityWarning {
                key: "wedding".into(),
                message: "tag 'W' is already held by account 'work'; using the stored tag".into(),
            }]
        );
    }

    #[test]
    fn configured_tag_matching_own_stored_tag_is_not_a_conflict() {
        let mut work = input("Work", Some("W"));
        work.config_tag = Some("W".into());
        let resolution = resolve_identities(&[work]).unwrap();
        assert_eq!(resolution.resolved[0].identity.tag, "W");
        assert!(resolution.warnings.is_empty());
    }

    #[test]
    fn an_invalid_configured_tag_is_ignored_and_names_the_key() {
        let mut work = input("Work", Some("W"));
        work.key = Some("work".into());
        work.config_tag = Some("WORK".into());
        let resolution = resolve_identities(&[work]).unwrap();
        assert_eq!(resolution.resolved[0].identity.tag, "W");
        assert_eq!(resolution.warnings.len(), 1);
        assert_eq!(resolution.warnings[0].key, "work");
        assert!(resolution.warnings[0].message.contains("1 to 3"));
    }

    #[test]
    fn an_invalid_configured_colour_is_ignored_and_lists_the_slots() {
        let mut work = input("Work", Some("W"));
        work.key = Some("work".into());
        work.stored_color = Some(AccountColor::Green);
        work.config_color = Some("teal".into());
        let resolution = resolve_identities(&[work]).unwrap();
        assert_eq!(resolution.resolved[0].identity.color, AccountColor::Green);
        let message = &resolution.warnings[0].message;
        assert!(message.contains("'teal'"), "{message}");
        assert!(message.contains("sky, amber, green"), "{message}");
    }

    #[test]
    fn a_duplicate_configured_tag_is_ignored_on_the_later_account() {
        let mut first = input("Work", None);
        first.config_tag = Some("OFF".into());
        let mut second = input("Travel", None);
        second.key = Some("travel".into());
        second.config_tag = Some("OFF".into());
        let resolution = resolve_identities(&[first, second]).unwrap();
        assert_eq!(resolution.resolved[0].identity.tag, "OFF");
        assert_eq!(resolution.resolved[1].identity.tag, "T");
        assert_eq!(resolution.warnings[0].key, "travel");
        assert!(resolution.warnings[0]
            .message
            .contains("already set on account 'Work'"));
    }

    #[test]
    fn provisional_tag_avoids_stored_and_configured_tags() {
        let work = input("Work", Some("W"));
        let mut travel = input("Travel", None);
        travel.config_tag = Some("T".into());
        let mut tango = input("Tango", None);
        tango.name = "Tango".into();
        let resolution = resolve_identities(&[work, travel, tango]).unwrap();
        // "T" is configured on Travel, so Travel's derived tag moves on to TR.
        assert_eq!(resolution.resolved[1].stored_tag, "TR");
        assert_eq!(resolution.resolved[2].stored_tag, "TA");
    }

    #[test]
    fn missing_stored_colour_takes_the_first_free_slot_in_input_order() {
        let mut first = input("Work", Some("W"));
        first.stored_color = Some(AccountColor::Sky);
        let second = input("Wedding", None);
        let resolution = resolve_identities(&[first, second]).unwrap();
        assert_eq!(resolution.resolved[1].identity.color, AccountColor::Amber);
    }

    #[test]
    fn config_colour_overrides_the_stored_colour_and_nothing_moves_it() {
        let mut work = input("Work", Some("W"));
        work.stored_color = Some(AccountColor::Green);
        work.config_color = Some("slate".into());
        let resolution = resolve_identities(&[work]).unwrap();
        assert_eq!(resolution.resolved[0].identity.color, AccountColor::Slate);
    }

    #[test]
    fn tag_holder_finds_a_stored_or_configured_holder() {
        let work = resolved("Work", "OFF", "W");
        let configured = [("Personal".to_string(), "Q".to_string())];
        assert_eq!(
            tag_holder("OFF", std::slice::from_ref(&work), &configured),
            Some("Work")
        );
        assert_eq!(
            tag_holder("W", std::slice::from_ref(&work), &configured),
            Some("Work")
        );
        assert_eq!(
            tag_holder("Q", std::slice::from_ref(&work), &configured),
            Some("Personal")
        );
        assert_eq!(
            tag_holder("PE", std::slice::from_ref(&work), &configured),
            None
        );
    }

    #[test]
    fn plan_refuses_a_clash_and_names_the_holder_only_when_allowed() {
        let work = resolved("Work", "W", "W");
        let wedding = resolved("Wedding", "WE", "WE");
        let error = plan_identity_change(
            &wedding,
            None,
            Some("W"),
            std::slice::from_ref(&work),
            &[],
            true,
        )
        .unwrap_err();
        assert_eq!(error, "Tag 'W' is already used by account 'Work'.");
        let error = plan_identity_change(
            &wedding,
            None,
            Some("W"),
            std::slice::from_ref(&work),
            &[],
            false,
        )
        .unwrap_err();
        assert_eq!(error, "Tag 'W' is already used by another account.");
        let (before, after) = plan_identity_change(
            &wedding,
            Some(AccountColor::Green),
            None,
            &[work],
            &[],
            true,
        )
        .unwrap();
        assert_eq!(before.color, AccountColor::Sky);
        assert_eq!(after.color, AccountColor::Green);
        assert_eq!(after.tag, "WE");
    }

    #[test]
    fn a_tag_configured_on_another_account_is_refused_and_hidden_from_a_scoped_caller() {
        // Personal is configured with Q but has no stored row yet, so only
        // config holds Q. The scoped refusal must not name Personal.
        let wedding = resolved("Wedding", "WE", "WE");
        let configured = [("Personal".to_string(), "Q".to_string())];
        let scoped =
            plan_identity_change(&wedding, None, Some("Q"), &[], &configured, false).unwrap_err();
        assert_eq!(scoped, "Tag 'Q' is already used by another account.");
        assert!(!scoped.contains("Personal"), "{scoped}");
        let unscoped =
            plan_identity_change(&wedding, None, Some("Q"), &[], &configured, true).unwrap_err();
        assert_eq!(unscoped, "Tag 'Q' is already used by account 'Personal'.");
    }

    #[test]
    fn identity_edit_leaves_default_account_unset_and_other_fields_alone() {
        let mut config = mxr_config::MxrConfig::default();
        config.accounts.insert(
            "work".into(),
            mxr_config::AccountConfig {
                name: "Work".into(),
                email: "bk@work.example".into(),
                enabled: true,
                sync: None,
                send: None,
                color: Some("teal".into()),
                tag: None,
            },
        );
        assert_eq!(config.general.default_account, None);
        set_identity_in_config(&mut config, "work", Some(AccountColor::Amber), Some("w")).unwrap();
        assert_eq!(config.general.default_account, None);
        let work = &config.accounts["work"];
        assert_eq!(work.color.as_deref(), Some("amber"));
        assert_eq!(work.tag.as_deref(), Some("W"));
        assert_eq!(work.name, "Work");
        assert_eq!(work.email, "bk@work.example");
    }

    fn config_account(name: &str, tag: Option<&str>) -> mxr_config::AccountConfig {
        mxr_config::AccountConfig {
            name: name.into(),
            email: format!("{}@example.com", name.to_lowercase()),
            enabled: true,
            sync: None,
            send: None,
            color: None,
            tag: tag.map(str::to_string),
        }
    }

    fn row_for(key: &str, account: &mxr_config::AccountConfig) -> mxr_core::Account {
        mxr_core::Account {
            id: crate::handler::config_account_id(key, account),
            name: account.name.clone(),
            email: account.email.clone(),
            sync_backend: None,
            send_backend: None,
            enabled: true,
        }
    }

    fn tag_for(resolution: &Resolution, id: &AccountId) -> (String, String) {
        let resolved = resolution
            .resolved
            .iter()
            .find(|resolved| &resolved.account_id == id)
            .unwrap();
        (resolved.identity.tag.clone(), resolved.stored_tag.clone())
    }

    #[tokio::test]
    async fn a_rename_a_removal_an_override_and_a_new_account_leave_other_tags_alone() {
        let store = mxr_store::Store::in_memory().await.unwrap();
        let work = mxr_core::Account {
            id: AccountId::new(),
            name: "Work".into(),
            email: "work@example.com".into(),
            sync_backend: None,
            send_backend: None,
            enabled: true,
        };
        let wedding = mxr_core::Account {
            name: "Wedding".into(),
            id: AccountId::new(),
            ..work.clone()
        };
        let travel_config = config_account("Travel", Some("OFF"));
        let travel = row_for("travel", &travel_config);
        store.insert_account(&work).await.unwrap();
        store.insert_account(&wedding).await.unwrap();
        store.insert_account(&travel).await.unwrap();

        // Rename Work, remove Wedding, then add a fourth account in the same
        // second as the others, which could tie on creation time.
        store
            .insert_account(&mxr_core::Account {
                name: "Office".into(),
                ..work.clone()
            })
            .await
            .unwrap();
        store.delete_account(&wedding.id).await.unwrap();
        let fourth = mxr_core::Account {
            name: "Wedding".into(),
            id: AccountId::new(),
            ..work.clone()
        };
        store
            .insert_account_reserving(&fourth, &["OFF".to_string()])
            .await
            .unwrap();

        let mut config = mxr_config::MxrConfig::default();
        config.accounts.insert("travel".into(), travel_config);
        let resolution = resolve_from_store(&store, &config).await.unwrap();

        assert_eq!(tag_for(&resolution, &work.id), ("W".into(), "W".into()));
        assert_eq!(tag_for(&resolution, &travel.id), ("OFF".into(), "T".into()));
        assert_eq!(tag_for(&resolution, &fourth.id), ("WE".into(), "WE".into()));
        assert!(resolution.warnings.is_empty());
    }

    #[tokio::test]
    async fn an_unstored_config_account_shows_the_colour_and_tag_insertion_stores() {
        // Config-only accounts have no row yet. The colour and tag shown before
        // insertion must be the ones insertion stores, so startup inserts them in
        // key order, as the resolver counts them.
        let store = mxr_store::Store::in_memory().await.unwrap();
        let mut config = mxr_config::MxrConfig::default();
        config
            .accounts
            .insert("wedding".into(), config_account("Wedding", None));
        config
            .accounts
            .insert("work".into(), config_account("Work", None));
        let before = resolve_from_store(&store, &config).await.unwrap();

        let reserved: Vec<String> = config
            .accounts
            .values()
            .filter_map(|a| a.tag.clone())
            .collect();
        let mut keys: Vec<&String> = config.accounts.keys().collect();
        keys.sort();
        for key in keys {
            let account = &config.accounts[key];
            store
                .insert_account_reserving(&row_for(key, account), &reserved)
                .await
                .unwrap();
        }
        let after = resolve_from_store(&store, &config).await.unwrap();

        for resolved in &before.resolved {
            let stored = after
                .resolved
                .iter()
                .find(|other| other.account_id == resolved.account_id)
                .unwrap();
            assert_eq!(
                resolved.identity.color, stored.identity.color,
                "{}",
                resolved.name
            );
            assert_eq!(
                resolved.identity.tag, stored.identity.tag,
                "{}",
                resolved.name
            );
        }
        // Sorted by key, "wedding" is first, so it takes sky.
        assert_eq!(before.resolved[0].identity.color, AccountColor::Sky);
    }
}
