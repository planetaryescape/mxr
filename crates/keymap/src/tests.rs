use super::*;
use std::path::PathBuf;

fn repo_file(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// The definition the web app is tested against. Regenerate with
/// `UPDATE_KEYMAP=1 cargo test -p mxr-keymap`.
const WEB_DATA: &str = "apps/web/src/lib/actions/keymap.generated.json";

#[test]
fn the_shipped_definition_is_sound() {
    assert_eq!(defaults().problems(), Vec::<String>::new());
}

#[test]
fn the_tui_keys_are_what_its_dispatcher_does() {
    // Written by the TUI's dispatcher probe (crates/tui/src/runner/tests/keymap.rs).
    let probed: BTreeMap<String, BTreeMap<String, String>> = serde_json::from_str(
        &std::fs::read_to_string(repo_file("docs/reference/tui-keymap.json"))
            .expect("docs/reference/tui-keymap.json is readable"),
    )
    .expect("tui-keymap.json is a context → key → action map");
    let defined = defaults().tui_keymap();
    let mut differences = Vec::new();
    for context in TUI_CONTEXTS {
        let empty = BTreeMap::new();
        let probe = probed.get(context).unwrap_or(&empty);
        let def = defined.get(context).unwrap_or(&empty);
        let keys: BTreeSet<&String> = probe.keys().chain(def.keys()).collect();
        for key in keys {
            let (p, d) = (probe.get(key), def.get(key));
            if p != d {
                differences.push(format!(
                    "{context} `{key}`: TUI does {p:?}, keymap.toml says {d:?}"
                ));
            }
        }
    }
    assert!(
        differences.is_empty(),
        "crates/keymap/keymap.toml disagrees with the TUI:\n{}",
        differences.join("\n")
    );
}

#[test]
fn the_web_data_is_generated_from_the_definition() {
    let path = repo_file(WEB_DATA);
    let fresh = format!(
        "{}\n",
        serde_json::to_string_pretty(defaults()).expect("the keymap serialises")
    );
    if std::env::var_os("UPDATE_KEYMAP").is_some() {
        std::fs::write(&path, &fresh).expect("write the web keymap data");
        return;
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        current == fresh,
        "{WEB_DATA} is stale; run `UPDATE_KEYMAP=1 cargo test -p mxr-keymap`"
    );
}

#[test]
fn contexts_are_all_or_a_list() {
    let keymap = Keymap::parse(
        r#"
        [[action]]
        id = "a"
        label = "A"
        group = "Mail"
        [[action.tui]]
        name = "A"
        keys = ["a"]
        contexts = "all"
        [[action.tui]]
        name = "B"
        keys = ["b"]
        contexts = ["list"]
        "#,
    )
    .expect("parses");
    assert_eq!(keymap.actions[0].tui[0].contexts, Contexts::All);
    assert!(keymap.actions[0].tui[1].contexts.includes("list"));
    assert!(!keymap.actions[0].tui[1].contexts.includes("reader"));

    let bad = Keymap::parse(
        r#"
        [[action]]
        id = "a"
        label = "A"
        group = "Mail"
        [[action.tui]]
        name = "A"
        keys = ["a"]
        contexts = "everywhere"
        "#,
    );
    assert!(bad.is_err());
}

#[test]
fn a_key_on_two_actions_or_shadowing_a_sequence_is_a_problem() {
    let keymap = Keymap::parse(
        r#"
        [[action]]
        id = "mail.archive"
        label = "Archive"
        group = "Mail"
        scopes = ["list"]
        keys = ["e"]

        [[action]]
        id = "mail.done"
        label = "Done"
        group = "Mail"
        scopes = ["list"]
        keys = ["e", "g"]

        [[action]]
        id = "nav.inbox"
        label = "Inbox"
        group = "Navigate"
        scopes = ["list"]
        keys = ["g i"]
        "#,
    )
    .expect("parses");
    assert_eq!(
        keymap.problems(),
        vec![
            "web scope `list`: `e` is bound to mail.archive, mail.done".to_string(),
            "web scope `list`: `g` is also the start of `g i`".to_string(),
        ]
    );
}

#[test]
fn a_tui_only_action_needs_a_tui_binding_and_no_web_keys() {
    let keymap = Keymap::parse(
        r#"
        [[action]]
        id = "tui.quit"
        label = "Quit"
        group = "TUI"
        clients = ["tui"]
        keys = ["q"]
        "#,
    )
    .expect("parses");
    assert_eq!(
        keymap.problems(),
        vec![
            "tui.quit: has web keys but is not a web action".to_string(),
            "tui.quit: is a TUI action with no TUI binding".to_string(),
        ]
    );
}
