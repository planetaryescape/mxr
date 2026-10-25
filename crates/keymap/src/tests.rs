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
fn compose_proof_keys_are_registered_only_for_the_proof_overlay() {
    let contexts = defaults().tui_keymap();
    let proof = contexts
        .get("compose-assist-proof")
        .expect("proof overlay has its own TUI key context");
    for (key, action) in [
        ("y", "ComposeProofAccept"),
        ("e", "ComposeProofAcceptEdit"),
        ("n", "ComposeProofRevise"),
        ("Escape", "ComposeProofKeep"),
        ("ArrowLeft", "ComposeProofPrevious"),
        ("ArrowRight", "ComposeProofNext"),
        ("ArrowUp", "ComposeProofScrollUp"),
        ("ArrowDown", "ComposeProofScrollDown"),
        ("PageUp", "ComposeProofPageUp"),
        ("PageDown", "ComposeProofPageDown"),
        ("Home", "ComposeProofScrollTop"),
        ("End", "ComposeProofScrollBottom"),
    ] {
        assert_eq!(proof.get(key).map(String::as_str), Some(action), "{key}");
    }
    for context in TUI_CONTEXTS {
        assert!(
            !contexts.get(context).is_some_and(|bindings| bindings
                .values()
                .any(|name| name.starts_with("ComposeProof"))),
            "proof overlay actions leaked into mail context {context}"
        );
    }
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

/// The Raycast extension's keys. Regenerated with `UPDATE_KEYMAP=1` too.
const RAYCAST_DATA: &str = "apps/raycast/src/keymap.generated.json";

#[test]
fn the_raycast_data_is_generated_from_the_definition() {
    let path = repo_file(RAYCAST_DATA);
    let fresh = format!(
        "{}\n",
        serde_json::to_string_pretty(&defaults().raycast_keys()).expect("the keys serialise")
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

#[test]
fn raycast_keys_are_cmd_and_the_web_letter_unless_the_definition_says() {
    let keymap = defaults();
    let key = |id: &str| keymap.get(id).and_then(KeyAction::raycast_key);
    assert_eq!(key("mail.reply").as_deref(), Some("Cmd+r"));
    assert_eq!(key("mail.mark-unread").as_deref(), Some("Cmd+Shift+u"));
    assert_eq!(
        key("todo.done").as_deref(),
        Some("Cmd+e"),
        "the first letter key, after Space"
    );
    assert_eq!(key("mail.undo").as_deref(), Some("Cmd+z"));
    assert_eq!(key("mail.reply-all").as_deref(), Some("Cmd+Shift+r"));
    assert_eq!(key("list.up"), None, "Cmd+K is Raycast's action panel");
    assert_eq!(
        RaycastShortcut::parse("Cmd+Shift+e"),
        Some(RaycastShortcut {
            modifiers: vec!["cmd".into(), "shift".into()],
            key: "e".into()
        })
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

#[test]
fn the_tui_lookup_finds_keys_and_sequences_in_their_context() {
    let keymap = defaults();
    let (action, binding) = keymap
        .tui_binding("todo", "e")
        .expect("e is bound in To do");
    assert_eq!(
        (action.id.as_str(), binding.name.as_str()),
        ("todo.done", "TodoDone")
    );
    assert!(keymap.tui_binding("todo", "g x").is_some());
    assert!(keymap.tui_starts_sequence("todo", "g"));
    assert!(!keymap.tui_starts_sequence("todo", "e"));
    assert!(keymap.tui_binding("screener", "e").is_none());
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

#[test]
fn unusable_remaps_are_reported_and_left_out() {
    let (remaps, problems) = remaps(
        r#"
        [todo]
        "Ctrl+i" = "todo.done"
        "q" = "todo.nope"
        "Ctrl+Shift+d" = "todo.done"
        "d" = "todo.done"
        "d d" = "todo.edit"
        [desk]
        "e" = "todo.done"
        "#,
    );
    assert_eq!(remaps.lookup("todo", "q"), None);
    assert_eq!(remaps.lookup("todo", "Ctrl+i"), None);
    let joined = problems.join("\n");
    for expected in [
        "`Ctrl+i`: terminals send this as Tab",
        "no action todo.nope",
        "Ctrl+Shift+ as plain Ctrl+",
        "`d` is also the start of `d d`",
        "[desk] is not a keys.toml section",
    ] {
        assert!(
            joined.contains(expected),
            "missing {expected:?} in:\n{joined}"
        );
    }
}

#[test]
fn binding_a_sequence_prefix_alone_is_reported() {
    let (_, problems) = remaps("[todo]\n\"g\" = \"todo.done\"\n");
    assert!(
        problems
            .iter()
            .any(|p| p.contains("starts default sequences")),
        "{problems:?}"
    );
}

#[test]
fn a_moved_key_must_be_free_where_it_moved_and_expires() {
    let keymap = Keymap::parse(
        r#"
        [[action]]
        id = "updates.let-go-all"
        label = "Let go of all"
        group = "Triage"
        scopes = ["updates"]
        keys = ["E"]
        [[action.moved]]
        key = "A"
        contexts = ["updates"]
        until = "99.0.0"

        [[action]]
        id = "updates.attach"
        label = "Attachments"
        group = "Mail"
        scopes = ["updates"]
        keys = ["A"]
        "#,
    )
    .expect("parses");
    assert_eq!(
        keymap.moved_from("updates", "A").map(|a| a.id.as_str()),
        Some("updates.let-go-all")
    );
    assert_eq!(keymap.moved_from("now", "A"), None);
    assert_eq!(
        keymap.problems(),
        vec!["updates.let-go-all: `A` moved away in updates but is still bound there".to_string()]
    );
    let expired = Keymap::parse(
        r#"
        [[action]]
        id = "x"
        label = "X"
        group = "Mail"
        keys = ["E"]
        [[action.moved]]
        key = "A"
        contexts = ["list"]
        until = "0.0.1"
        "#,
    )
    .expect("parses");
    assert!(expired.problems()[0].contains("delete the entry"));
}

#[test]
fn a_moved_key_can_name_another_actions_key() {
    let keymap = Keymap::parse(
        r#"
        [[action]]
        id = "nav.tab-mail"
        label = "Mail"
        group = "Navigate"
        [[action.moved]]
        key = "1"
        contexts = ["list"]
        until = "99.0.0"
        to = "nav.inbox"

        [[action]]
        id = "nav.inbox"
        label = "Go to Inbox"
        group = "Navigate"
        keys = ["g i"]

        [[action]]
        id = "tui.center-current"
        label = "Center current"
        group = "TUI"
        clients = ["tui"]
        [[action.moved]]
        key = "z z"
        contexts = ["list"]
        until = "99.0.0"
        "#,
    )
    .expect("parses");
    assert_eq!(
        keymap.problems(),
        Vec::<String>::new(),
        "a keyless action may keep its moved keys"
    );
    let (action, moved) = keymap.moved_entry("list", "1").expect("1 moved");
    assert_eq!(action.id, "nav.tab-mail");
    assert_eq!(keymap.moved_target(action, moved).id, "nav.inbox");
    assert!(keymap.moved_starts_sequence("list", "z"));
    assert!(!keymap.moved_starts_sequence("now", "z"));

    let dangling = Keymap::parse(
        r#"
        [[action]]
        id = "x"
        label = "X"
        group = "Mail"
        [[action.moved]]
        key = "1"
        contexts = ["list"]
        until = "99.0.0"
        to = "nowhere"
        "#,
    )
    .expect("parses");
    assert_eq!(
        dangling.problems(),
        vec!["x: the move of `1` points to no action `nowhere`".to_string()]
    );
}

#[test]
fn a_tui_only_move_may_keep_a_key_the_web_binds_there() {
    let keymap = Keymap::parse(
        r#"
        [[action]]
        id = "reader.toggle-message"
        label = "Expand or collapse"
        group = "Read"
        scopes = ["reader"]
        keys = ["o"]
        clients = ["web"]

        [[action]]
        id = "tui.reader-open-original"
        label = "Open in browser"
        group = "Read"
        clients = ["tui"]
        [[action.moved]]
        key = "o"
        contexts = ["reader"]
        until = "99.0.0"

        [[action]]
        id = "rules.new"
        label = "New rule"
        group = "Rules"
        palette_only = true
        [[action.moved]]
        key = "n"
        contexts = ["rules"]
        until = "99.0.0"
        to_key = "c"
        "#,
    )
    .expect("parses");
    assert_eq!(keymap.problems(), Vec::<String>::new());
    let (_, moved) = keymap.moved_entry("rules", "n").expect("n moved");
    assert_eq!(moved.to_key.as_deref(), Some("c"));

    let web = Keymap::parse(
        r#"
        [[action]]
        id = "reader.toggle-message"
        label = "Expand or collapse"
        group = "Read"
        scopes = ["reader"]
        keys = ["o"]
        clients = ["web"]

        [[action]]
        id = "reader.open-original"
        label = "Open in browser"
        group = "Read"
        clients = ["web"]
        [[action.moved]]
        key = "o"
        contexts = ["reader"]
        until = "99.0.0"
        "#,
    )
    .expect("parses");
    assert_eq!(
        web.problems(),
        vec!["reader.open-original: `o` moved away in reader but is still bound there".to_string()]
    );
}

#[test]
fn a_moved_key_names_a_known_place() {
    let keymap = Keymap::parse(
        r##"
        [[action]]
        id = "tui.drafts-delete"
        label = "Delete draft"
        group = "Compose"
        clients = ["tui"]
        [[action.moved]]
        key = "d"
        contexts = ["drafts", "draft"]
        until = "99.0.0"
        to_key = "#"
        "##,
    )
    .expect("parses");
    assert_eq!(
        keymap.problems(),
        vec!["tui.drafts-delete: the move of `d` names unknown context `draft`".to_string()]
    );
}
