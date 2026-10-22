//! Resolves each account's colour and tag for the mail marks (blueprint 33).
//!
//! The store keeps each account's colour and tag from the moment it is
//! created. Config can override either one. The pure functions here decide
//! the rules. `resolve_from_store` only gathers the stored values and the
//! config into the shape they take.

use crate::state::AppState;
use mxr_core::{derive_account_tag, next_account_color, AccountColor, AccountId, NoFreeAccountTag};
use mxr_protocol::AccountIdentityData;
use std::collections::{HashMap, HashSet};

/// One account in creation order: what the store holds and what config says.
/// A stored value is `None` only for an account with no row yet.
#[derive(Debug, Clone)]
pub struct IdentityInput {
    pub account_id: AccountId,
    pub name: String,
    pub stored_color: Option<AccountColor>,
    pub stored_tag: Option<String>,
    pub config_color: Option<AccountColor>,
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

/// A configured tag that another account already holds. The configured tag is
/// not applied, and the account keeps its stored tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityConflict {
    pub account_name: String,
    pub configured_tag: String,
    pub holder_name: String,
}

#[derive(Debug, Clone)]
pub struct Resolution {
    /// Same order as the input.
    pub resolved: Vec<ResolvedIdentity>,
    pub conflicts: Vec<IdentityConflict>,
}

/// Resolves every account's colour and tag.
///
/// Colour is the config override, else the stored colour. Tag is the config
/// override, else the stored tag. A stored tag is never derived again, so a
/// rename, a removal or an override on one account cannot change another's.
///
/// A config tag equal to another account's stored tag is refused and reported
/// as a conflict. The override is dropped and the account keeps its stored tag.
///
/// An account with no stored value gets the one insertion will give it:
/// `next_account_color` and a derived tag, both against what is stored and
/// the configured tags, counting accounts in input order. Callers insert such
/// accounts in that order, so the value shown is the one stored.
pub fn resolve_identities(inputs: &[IdentityInput]) -> Result<Resolution, NoFreeAccountTag> {
    let reserved: HashSet<String> = inputs
        .iter()
        .filter_map(|input| input.config_tag.clone())
        .collect();
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

    let mut resolved = Vec::with_capacity(inputs.len());
    let mut conflicts = Vec::new();
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

        let config_tag = match &input.config_tag {
            Some(configured) => match holders.get(configured.as_str()) {
                Some(&holder) if holder != index => {
                    conflicts.push(IdentityConflict {
                        account_name: input.name.clone(),
                        configured_tag: configured.clone(),
                        holder_name: inputs[holder].name.clone(),
                    });
                    None
                }
                _ => Some(configured.clone()),
            },
            None => None,
        };

        resolved.push(ResolvedIdentity {
            account_id: input.account_id.clone(),
            name: input.name.clone(),
            identity: AccountIdentityData {
                color: input.config_color.unwrap_or(stored_color),
                tag: config_tag.unwrap_or_else(|| stored_tag.clone()),
            },
            stored_tag,
        });
    }
    Ok(Resolution {
        resolved,
        conflicts,
    })
}

/// The colour override from config. Validation at load already refused any
/// other name, so a name that fails to parse here is treated as no override.
pub fn config_color(account: &mxr_config::AccountConfig) -> Option<AccountColor> {
    account.color.as_deref().and_then(|raw| raw.parse().ok())
}

/// The other account that would show `tag` or already stores it, if any.
/// A stored tag is reserved even when that account overrides it in config.
pub fn tag_clash<'a>(
    others: impl IntoIterator<Item = &'a ResolvedIdentity>,
    tag: &str,
) -> Option<&'a ResolvedIdentity> {
    others
        .into_iter()
        .find(|other| other.identity.tag == tag || other.stored_tag == tag)
}

/// Checks a requested colour and tag for one account and returns the identity
/// before and after. `others` are the other accounts. A dry run and the real
/// write both call this, so they agree.
pub fn plan_identity_change(
    current: &ResolvedIdentity,
    color: Option<AccountColor>,
    tag: Option<&str>,
    others: &[ResolvedIdentity],
) -> Result<(AccountIdentityData, AccountIdentityData), String> {
    if let Some(tag) = tag {
        if let Some(clash) = tag_clash(others, tag) {
            return Err(format!(
                "Tag '{tag}' is already used by account '{}'.",
                clash.name
            ));
        }
    }
    let after = AccountIdentityData {
        color: color.unwrap_or(current.identity.color),
        tag: tag
            .map(str::to_string)
            .unwrap_or_else(|| current.identity.tag.clone()),
    };
    Ok((current.identity.clone(), after))
}

/// Writes a colour and tag override into `config` for one account. It touches
/// nothing else: `default_account` and the account's other fields are left as
/// they were.
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
        account.tag = Some(mxr_core::parse_account_tag(tag).map_err(|e| e.to_string())?);
    }
    mxr_config::validate_account_identities(&mut config.accounts).map_err(|e| e.to_string())
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
        let (name, config_color, config_tag) = match configured {
            Some((_, account)) => (
                account.name.clone(),
                config_color(account),
                account.tag.clone(),
            ),
            None => (slot.name.clone(), None, None),
        };
        seen.insert(slot.account_id.clone());
        inputs.push(IdentityInput {
            account_id: slot.account_id,
            name,
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
    unstored.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (key, account) in unstored {
        inputs.push(IdentityInput {
            account_id: crate::handler::config_account_id(key, account),
            name: account.name.clone(),
            stored_color: None,
            stored_tag: None,
            config_color: config_color(account),
            config_tag: account.tag.clone(),
        });
    }

    resolve_identities(&inputs).map_err(|error| error.to_string())
}

pub async fn resolve_for_state(state: &AppState) -> Result<Resolution, String> {
    resolve_from_store(&state.store, &state.config_snapshot()).await
}

/// Logs each refused override. Called where config is loaded or reloaded, so
/// the operator sees why a configured tag is not in effect.
pub fn log_conflicts(conflicts: &[IdentityConflict]) {
    for conflict in conflicts {
        tracing::error!(
            account = %conflict.account_name,
            tag = %conflict.configured_tag,
            holder = %conflict.holder_name,
            "configured account tag is already held by another account; keeping the stored tag"
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
            stored_color: None,
            stored_tag: stored_tag.map(str::to_string),
            config_color: None,
            config_tag: None,
        }
    }

    #[test]
    fn stored_tag_is_used_even_after_a_rename() {
        let mut office = input("Office", Some("W"));
        office.stored_color = Some(AccountColor::Sky);
        let resolved = resolve_identities(&[office]).unwrap();
        assert_eq!(resolved.resolved[0].identity.tag, "W");
        assert_eq!(resolved.resolved[0].stored_tag, "W");
    }

    #[test]
    fn config_tag_wins_over_the_stored_tag() {
        let mut work = input("Work", Some("W"));
        work.config_tag = Some("OFF".into());
        let resolved = resolve_identities(&[work]).unwrap();
        assert_eq!(resolved.resolved[0].identity.tag, "OFF");
        assert_eq!(resolved.resolved[0].stored_tag, "W");
        assert!(resolved.conflicts.is_empty());
    }

    #[test]
    fn configured_tag_held_by_another_stored_account_is_refused() {
        // Work stores W. Wedding's override asks for W too: refused, Wedding
        // keeps WE, and the conflict names both accounts.
        let work = input("Work", Some("W"));
        let mut wedding = input("Wedding", Some("WE"));
        wedding.config_tag = Some("W".into());
        let resolved = resolve_identities(&[work, wedding]).unwrap();
        assert_eq!(resolved.resolved[1].identity.tag, "WE");
        assert_eq!(resolved.resolved[0].identity.tag, "W");
        assert_eq!(
            resolved.conflicts,
            vec![IdentityConflict {
                account_name: "Wedding".into(),
                configured_tag: "W".into(),
                holder_name: "Work".into(),
            }]
        );
    }

    #[test]
    fn configured_tag_matching_own_stored_tag_is_not_a_conflict() {
        let mut work = input("Work", Some("W"));
        work.config_tag = Some("W".into());
        let resolved = resolve_identities(&[work]).unwrap();
        assert_eq!(resolved.resolved[0].identity.tag, "W");
        assert!(resolved.conflicts.is_empty());
    }

    #[test]
    fn provisional_tag_avoids_stored_and_configured_tags() {
        let work = input("Work", Some("W"));
        let mut travel = input("Travel", None);
        travel.config_tag = Some("T".into());
        let mut wedding = input("Wedding", None);
        wedding.name = "Tango".into();
        let resolved = resolve_identities(&[work, travel, wedding]).unwrap();
        // "Travel" wants T, which is configured on Travel itself: the derived
        // tag is the initial, so it is taken by configuration and the search
        // moves on to TR.
        assert_eq!(resolved.resolved[1].stored_tag, "TR");
        assert_eq!(resolved.resolved[2].stored_tag, "TA");
    }

    #[test]
    fn missing_stored_colour_takes_the_first_free_slot_in_input_order() {
        let mut first = input("Work", Some("W"));
        first.stored_color = Some(AccountColor::Sky);
        let second = input("Wedding", None);
        let resolved = resolve_identities(&[first, second]).unwrap();
        assert_eq!(resolved.resolved[1].identity.color, AccountColor::Amber);
    }

    #[test]
    fn config_colour_overrides_the_stored_colour_and_nothing_moves_it() {
        let mut work = input("Work", Some("W"));
        work.stored_color = Some(AccountColor::Green);
        work.config_color = Some(AccountColor::Slate);
        let resolved = resolve_identities(&[work]).unwrap();
        assert_eq!(resolved.resolved[0].identity.color, AccountColor::Slate);
    }

    #[test]
    fn clash_is_checked_against_stored_and_resolved_tags() {
        let work = ResolvedIdentity {
            account_id: AccountId::new(),
            name: "Work".into(),
            identity: AccountIdentityData {
                color: AccountColor::Sky,
                tag: "OFF".into(),
            },
            stored_tag: "W".into(),
        };
        assert_eq!(tag_clash([&work], "OFF").unwrap().name, "Work");
        assert_eq!(tag_clash([&work], "W").unwrap().name, "Work");
        assert!(tag_clash([&work], "PE").is_none());
    }

    #[test]
    fn plan_refuses_a_clash_and_names_the_holder() {
        let work = ResolvedIdentity {
            account_id: AccountId::new(),
            name: "Work".into(),
            identity: AccountIdentityData {
                color: AccountColor::Sky,
                tag: "W".into(),
            },
            stored_tag: "W".into(),
        };
        let wedding = ResolvedIdentity {
            account_id: AccountId::new(),
            name: "Wedding".into(),
            identity: AccountIdentityData {
                color: AccountColor::Amber,
                tag: "WE".into(),
            },
            stored_tag: "WE".into(),
        };
        let error = plan_identity_change(&wedding, None, Some("W"), std::slice::from_ref(&work))
            .unwrap_err();
        assert_eq!(error, "Tag 'W' is already used by account 'Work'.");
        let (before, after) =
            plan_identity_change(&wedding, Some(AccountColor::Green), None, &[work]).unwrap();
        assert_eq!(before.color, AccountColor::Amber);
        assert_eq!(after.color, AccountColor::Green);
        assert_eq!(after.tag, "WE");
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
                color: None,
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
        assert!(resolution.conflicts.is_empty());
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
        // Sorted by key, "wedding" is first, so it takes sky and WE stays free for it.
        assert_eq!(before.resolved[0].identity.color, AccountColor::Sky);
    }
}
