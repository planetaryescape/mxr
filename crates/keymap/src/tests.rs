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
fn the_clients_use_the_same_keys() {
    let differences = defaults().client_differences();
    assert!(
        differences.is_empty(),
        "the web app and the TUI differ:\n{}",
        differences.join("\n")
    );
}

#[test]
fn the_tui_keys_are_what_its_dispatcher_does() {
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
        serde_json::to_string_pretty(definition()).expect("the keymap serialises")
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

const RAYCAST_DATA: &str = "apps/raycast/src/keymap.generated.json";

#[test]
fn the_raycast_data_is_generated_from_the_definition() {
    let path = repo_file(RAYCAST_DATA);
    let fresh = format!(
        "{}\n",
        serde_json::to_string_pretty(&definition().raycast_keys()).expect("the keys serialise")
    );
    if std::env::var_os("UPDATE_KEYMAP").is_some() {
        std::fs::write(&path, &fresh).expect("write the Raycast keymap data");
        return;
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        current == fresh,
        "{RAYCAST_DATA} is stale; run `UPDATE_KEYMAP=1 cargo test -p mxr-keymap`"
    );
}

const KEYBINDINGS_DOC: &str = "site/src/content/docs/reference/keybindings.md";
const TUI_DOC_START: &str = "<!-- tui-keys:start (generated from crates/keymap/keymap.toml; run UPDATE_KEYMAP=1 cargo test -p mxr-keymap) -->";
const TUI_DOC_END: &str = "<!-- tui-keys:end -->";

#[test]
fn the_tui_keybindings_tables_are_generated_from_the_definition() {
    let path = repo_file(KEYBINDINGS_DOC);
    let doc = std::fs::read_to_string(&path).expect("keybindings.md is readable");
    let start = doc
        .find(TUI_DOC_START)
        .expect("keybindings.md has the TUI start marker");
    let end = doc
        .find(TUI_DOC_END)
        .expect("keybindings.md has the TUI end marker");
    let current = doc[start + TUI_DOC_START.len()..end].trim();
    let fresh = definition().tui_reference_markdown();
    if std::env::var_os("UPDATE_KEYMAP").is_some() && current != fresh {
        let updated = format!(
            "{}\n\n{fresh}\n\n{}",
            &doc[..start + TUI_DOC_START.len()],
            &doc[end..]
        );
        std::fs::write(&path, updated).expect("write keybindings.md");
        return;
    }
    assert!(
        current == fresh,
        "{KEYBINDINGS_DOC}'s TUI tables are stale; run `UPDATE_KEYMAP=1 cargo test -p mxr-keymap`"
    );
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

fn remaps(text: &str) -> (ModeRemaps, Vec<String>) {
    ModeRemaps::from_table(&text.parse().expect("toml"), defaults())
}

#[test]
fn a_mode_section_remaps_keys_in_that_mode_only() {
    let (remaps, problems) = remaps(
        r#"
        [todo]
        "x" = "todo.done"
        "Z" = ""
        "#,
    );
    assert_eq!(problems, Vec::<String>::new());
    assert_eq!(remaps.lookup("todo", "x"), Some(Some("todo.done")));
    assert_eq!(remaps.lookup("todo", "Z"), Some(None));
    assert_eq!(remaps.lookup("now", "x"), None);
    assert!(remaps.retires("todo", "todo.done"));
    assert!(!remaps.retires("now", "now.done"));
    assert_eq!(remaps.key_for("todo", "todo.done"), Some("x"));
}

const EXPIRING: &str = r#"
[[action]]
id = "todo.catchup"
label = "Catch up"
group = "Navigate"
scopes = ["todo"]
keys = ["2"]

# Catch up left C in K3 wave 4b.
[[action.moved]]
key = "C"
contexts = ["todo"]
until = "0.6.197"

[[action.moved]]
key = "c"
contexts = ["todo"]
until = "0.6.200"

[[action]]
id = "tui.next-search-result"
label = "Next search result"
group = "TUI"
clients = ["tui"]

[[action.moved]]
key = "n"
contexts = ["list"]
until = "0.6.197"
"#;

#[test]
fn a_move_is_ignored_from_its_until_on() {
    let keymap = Keymap::parse(EXPIRING).expect("parses");
    let before = keymap.live_at("0.6.196");
    assert_eq!(before, keymap, "nothing has expired yet");
    assert!(before.moved_from("todo", "C").is_some());
    assert!(before.moved_from("list", "n").is_some());

    let after = keymap.live_at("0.6.197");
    assert_eq!(
        after.moved_from("todo", "C"),
        None,
        "the old key does nothing"
    );
    assert!(
        after.moved_from("todo", "c").is_some(),
        "a later until still shows"
    );
    assert_eq!(
        after.get("tui.next-search-result"),
        None,
        "a TUI action that only carried the move goes with it"
    );
    assert_eq!(after.problems(), Vec::<String>::new());
    assert_eq!(
        keymap.expired_moves("0.6.197"),
        vec![
            "todo.catchup: the move of `C` in todo expired at 0.6.197".to_string(),
            "tui.next-search-result: the move of `n` in list expired at 0.6.197".to_string(),
        ]
    );
    assert_eq!(keymap.expired_moves("0.6.196"), Vec::<String>::new());
    assert_eq!(keymap.problems(), Vec::<String>::new());
}

#[test]
fn no_release_fails_the_check_by_crossing_an_until() {
    let mut versions: Vec<&str> = definition()
        .actions
        .iter()
        .flat_map(|action| action.moved.iter().map(|moved| moved.until.as_str()))
        .collect();
    versions.push("999.0.0");
    for version in versions {
        let live = definition().live_at(version);
        assert_eq!(live.problems(), Vec::<String>::new(), "at {version}");
        assert_eq!(
            live.client_differences(),
            Vec::<String>::new(),
            "at {version}"
        );
    }
}

#[test]
fn the_prune_of_the_shipped_file_matches_what_the_clients_ignore() {
    for version in ["0.6.197", "0.6.200", "999.0.0"] {
        let pruned = prune_expired_moves(DEFAULT_KEYMAP, version).expect("prunes");
        assert_eq!(
            Keymap::parse(&pruned).expect("the pruned file parses"),
            definition().live_at(version),
            "at {version}"
        );
    }
}

fn keys_file(text: &str) -> (KeysFile, Vec<String>) {
    KeysFile::parse(text, defaults())
}

#[test]
fn keys_toml_reads_the_switch_and_action_keys() {
    let (file, problems) = keys_file(
        r#"
        single_keys = false

        [actions]
        "mail.archive" = ";"
        "mail.star" = ["Mod+e", "S"]
        "mail.trash" = []
        "#,
    );
    assert_eq!(problems, Vec::<String>::new());
    assert!(!file.single_keys);
    assert_eq!(file.actions["mail.archive"], vec![";"]);
    assert_eq!(file.actions["mail.star"], vec!["Mod+e", "S"]);
    assert_eq!(file.actions["mail.trash"], Vec::<String>::new());
    assert!(keys_file("").0.single_keys);
}

#[test]
fn an_action_key_replaces_its_keys_in_both_clients() {
    let actions = BTreeMap::from([("mail.archive".to_string(), vec!["%".to_string()])]);
    let remapped = defaults().remapped(&actions);
    assert_eq!(remapped.problems, Vec::<String>::new());
    assert_eq!(remapped.applied, actions);
    let archive = remapped
        .keymap
        .get("mail.archive")
        .expect("mail.archive is defined");
    assert_eq!(archive.keys, vec!["%"]);
    let tui = &remapped.keymap.tui_keymap();
    assert_eq!(tui["list"].get("%").map(String::as_str), Some("Archive"));
    assert_eq!(tui["list"].get("e"), None);
    assert_eq!(tui["reader"].get("%").map(String::as_str), Some("Archive"));
}

#[test]
fn edits_keep_the_rest_of_keys_toml() {
    let text = "# my keys\n[todo]\n\"x\" = \"todo.done\" # tick\n";
    let set = edit_keys_toml(
        text,
        &KeysEdit::SetAction {
            id: "mail.archive".into(),
            keys: vec![";".into()],
        },
    )
    .expect("the edit applies");
    assert!(set.contains("# my keys") && set.contains("# tick"), "{set}");
    assert!(set.contains("[actions]\n\"mail.archive\" = \";\""), "{set}");
    let several = edit_keys_toml(
        &set,
        &KeysEdit::SetAction {
            id: "mail.star".into(),
            keys: vec!["s".into(), "S".into()],
        },
    )
    .expect("the edit applies");
    assert!(
        several.contains("\"mail.star\" = [\"s\", \"S\"]"),
        "{several}"
    );
    let off = edit_keys_toml(&several, &KeysEdit::SingleKeys(false)).expect("the edit applies");
    let (file, problems) = keys_file(&off);
    assert_eq!(problems, Vec::<String>::new(), "{off}");
    assert!(!file.single_keys);
    assert_eq!(file.actions.len(), 2);
    let on = edit_keys_toml(&off, &KeysEdit::SingleKeys(true)).expect("the edit applies");
    assert!(!on.contains("single_keys"), "{on}");
    let reset = edit_keys_toml(
        &on,
        &KeysEdit::ResetAction {
            id: "mail.star".into(),
        },
    )
    .expect("the edit applies");
    let reset = edit_keys_toml(
        &reset,
        &KeysEdit::ResetAction {
            id: "mail.archive".into(),
        },
    )
    .expect("the edit applies");
    assert!(!reset.contains("[actions]"), "{reset}");
    assert!(reset.contains("# tick"), "{reset}");
    assert!(edit_keys_toml("[todo", &KeysEdit::SingleKeys(false)).is_err());
}
