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
fn the_key_check_derives_differences_from_the_definition() {
    let keymap = Keymap::parse(
        r#"
        [[action]]
        id = "palette"
        label = "Palette"
        group = "Navigate"
        keys = ["Mod+k", "Mod+a"]
        [[action.tui]]
        name = "OpenCommandPalette"
        keys = ["Ctrl+k"]
        contexts = ["list", "reader", "sidebar", "todo", "now", "messages", "archive", "updates", "reading", "focus"]

        [[action]]
        id = "select"
        label = "Select"
        group = "Select"
        scopes = ["list"]
        keys = ["x"]
        [[action.tui]]
        name = "ToggleSelect"
        keys = ["x"]
        contexts = ["list", "sidebar"]

        [[action]]
        id = "compose"
        label = "Compose"
        group = "Compose"
        keys = ["c"]

        [[action]]
        id = "pane"
        label = "Switch pane"
        group = "TUI"
        clients = ["tui"]
        [[action.tui]]
        name = "SwitchPane"
        keys = ["Tab"]
        contexts = ["list"]
        "#,
    )
    .expect("the test definition parses");
    // `Mod+k` is the TUI's `Ctrl+k`; `Mod+a` would be `Ctrl+a`, screen's
    // prefix. `c` is in the web only, and `Tab`'s action doesn't say why.
    let differences = keymap.client_differences();
    assert!(
        differences.iter().any(|d| d.starts_with("list `c`")),
        "{differences:?}"
    );
    assert!(
        differences.iter().any(|d| d.starts_with("sidebar `x`")),
        "{differences:?}"
    );
    assert!(differences
        .contains(&"pane: is in one client only, and has no `note` saying why".to_string()));
    assert!(
        !differences
            .iter()
            .any(|d| d.contains("`Mod+") || d.contains("`Ctrl+k`")),
        "{differences:?}"
    );

    let mut declared = keymap.clone();
    declared.actions[2].clients = vec![Client::Web];
    declared.actions[2].note = Some("the TUI composes elsewhere".into());
    declared.actions[3].note = Some("a browser keeps Tab".into());
    declared.tui_pass_through.push(TuiPassThrough {
        from: vec!["sidebar".into()],
        to: "list".into(),
        why: "the sidebar sits beside the list".into(),
    });
    declared.tui_pass_through.push(TuiPassThrough {
        from: vec!["reader".into()],
        to: "list".into(),
        why: "unused".into(),
    });
    assert_eq!(
        declared.client_differences(),
        vec!["the TUI pass-through from reader to list explains no key; delete it".to_string()]
    );
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
        ("o", "ComposeProofOpenSource"),
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

/// The Raycast extension's keys. Regenerated with `UPDATE_KEYMAP=1` too.
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

/// The page whose TUI tables are generated from the definition.
const KEYBINDINGS_DOC: &str = "site/src/content/docs/reference/keybindings.md";
const TUI_DOC_START: &str = "<!-- tui-keys:start (generated from crates/keymap/keymap.toml; run UPDATE_KEYMAP=1 cargo test -p mxr-keymap) -->";
const TUI_DOC_END: &str = "<!-- tui-keys:end -->";

#[test]
fn the_tui_keybindings_tables_are_generated_from_the_definition() {
    let path = repo_file(KEYBINDINGS_DOC);
    let doc = std::fs::read_to_string(&path).expect("keybindings.md is readable");
    // A missing marker means the generated block was deleted from the page.
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

/// A place with keys of its own gets a table on the page, so a new screen
/// can't go unpublished.
#[test]
fn every_place_with_tui_keys_has_a_table() {
    let keymap = defaults();
    let missing: Vec<&str> = TUI_CONTEXTS
        .iter()
        .chain(SCREEN_CONTEXTS.iter())
        .copied()
        .filter(|place| {
            keymap
                .actions
                .iter()
                .flat_map(|a| &a.tui)
                .any(|binding| !binding.keys.is_empty() && binding.contexts.includes(place))
        })
        .filter(|place| {
            !crate::docs::PLACES
                .iter()
                .any(|(listed, _)| listed == place)
                || place_title(place).is_none()
        })
        .collect();
    assert!(missing.is_empty(), "no table or title for {missing:?}");
    let missing: Vec<&str> = OVERLAY_CONTEXTS
        .into_iter()
        .filter(|place| {
            keymap
                .actions
                .iter()
                .flat_map(|a| &a.tui)
                .any(|binding| !binding.keys.is_empty() && binding.contexts.includes(place))
        })
        .filter(|place| {
            !crate::docs::OVERLAYS
                .iter()
                .any(|(listed, _)| listed == place)
                || place_title(place).is_none()
        })
        .collect();
    assert!(
        missing.is_empty(),
        "no overlay table or title for {missing:?}"
    );
}

/// An overlay reads its own keys, so an action lives in overlays only or
/// in none, only an overlay's web keys can be the browser's, and keys.toml
/// can't remap an overlay's key.
#[test]
fn overlay_keys_stay_in_overlays() {
    let keymap = Keymap::parse(
        r#"
        [[action]]
        id = "overlay.close"
        label = "Close"
        group = "Navigate"
        scopes = ["links"]
        keys = ["Escape"]
        web_native = true
        [[action.tui]]
        name = "inline:close"
        keys = ["Escape"]
        contexts = ["links"]

        [[action]]
        id = "mixed"
        label = "Mixed"
        group = "Navigate"
        scopes = ["list", "links"]
        keys = ["x"]
        web_native = true
        [[action.tui]]
        name = "ToggleSelect"
        keys = ["x"]
        contexts = ["list"]
        "#,
    )
    .expect("the test definition parses");
    assert!(keymap
        .get("overlay.close")
        .is_some_and(KeyAction::is_overlay));
    assert!(!keymap.get("mixed").is_some_and(KeyAction::is_overlay));
    let problems = keymap.problems();
    assert!(
        problems
            .iter()
            .any(|p| p.starts_with("mixed: `links` is an overlay")),
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.starts_with("mixed: only an overlay's web keys")),
        "{problems:?}"
    );
    assert!(!keymap.raycast_keys().contains_key("overlay.close"));
    assert!(
        action_keys_problems(&keymap, "overlay.close", &["c".into()])[0].contains("overlay's key"),
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

/// A palette-only TUI action names what the palette runs with a binding that
/// has no keys; a keyless binding anywhere else would be a key nobody can press.
#[test]
fn a_keyless_tui_binding_is_for_a_palette_only_action() {
    let keymap = Keymap::parse(
        r#"
        [[action]]
        id = "tui.config"
        label = "Edit config"
        group = "Settings"
        clients = ["tui"]
        palette_only = true

        [[action.tui]]
        name = "EditConfig"
        keys = []
        contexts = "all"

        [[action]]
        id = "tui.keyless"
        label = "Keyless"
        group = "TUI"
        clients = ["tui"]

        [[action.tui]]
        name = "Help"
        keys = []
        contexts = "all"

        [[action]]
        id = "tui.keyed"
        label = "Keyed"
        group = "TUI"
        clients = ["tui"]
        palette_only = true

        [[action.tui]]
        name = "Help"
        keys = ["?"]
        contexts = ["list"]
        "#,
    )
    .expect("parses");
    assert_eq!(
        keymap.problems(),
        vec![
            "tui.keyless: a TUI binding with no keys is for a palette-only action".to_string(),
            "tui.keyed: is palette-only but has keys".to_string(),
        ]
    );
    assert!(keymap.tui_binding("list", "").is_none());
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
fn a_moved_key_must_be_free_where_it_moved() {
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
}

/// The definition with one move announced until 0.6.197, beside another
/// until 0.6.200, and a TUI action that only carries a move.
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
    // Expiry is never a problem in the file as written either: the clients
    // ignore the entry, so no release waits on deleting it.
    assert_eq!(keymap.problems(), Vec::<String>::new());
}

#[test]
fn the_clients_use_the_definition_at_this_version() {
    assert_eq!(*defaults(), definition().live_at(VERSION));
}

/// Each release that crosses an `until` drops moves from the definition the
/// clients use. None of those releases may fail the definition check.
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
fn the_prune_deletes_exactly_the_expired_moves() {
    let pruned = prune_expired_moves(EXPIRING, "0.6.197").expect("prunes");
    assert_eq!(
        Keymap::parse(&pruned).expect("the pruned file parses"),
        Keymap::parse(EXPIRING).expect("parses").live_at("0.6.197")
    );
    assert!(!pruned.contains("Catch up left C"), "its comment goes too");
    assert!(pruned.contains("until = \"0.6.200\""), "{pruned}");
    assert!(!pruned.contains("tui.next-search-result"), "{pruned}");
    assert_eq!(
        prune_expired_moves(EXPIRING, "0.6.196").expect("prunes"),
        EXPIRING,
        "nothing expired, nothing changes"
    );
}

/// The prune of the shipped file at any version leaves what the clients
/// use at that version. The file may hold expired moves until the
/// scheduled prune runs, so this never compares it with the file itself.
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
fn unusable_action_keys_are_reported_and_left_out() {
    let (file, problems) = keys_file(
        r#"
        single_keys = "no"
        [actions]
        "mail.nope" = "x"
        "mail.archive" = "Mod+f"
        "mail.star" = "Ctrl+Shift+s"
        "mail.trash" = ["x", "x"]
        "mail.spam" = 3
        "#,
    );
    assert!(file.actions.is_empty(), "{:?}", file.actions);
    let joined = problems.join("\n");
    for expected in [
        "`single_keys` must be true or false",
        "mail.nope: no such action",
        "the browser keeps `Mod+f`",
        "Ctrl+Shift+ as plain Ctrl+",
        "`x` is listed twice",
        "mail.spam: give a key",
    ] {
        assert!(
            joined.contains(expected),
            "missing {expected:?} in:\n{joined}"
        );
    }
    // The mode-section check accepts the new section and setting.
    let (_, mode_problems) = remaps("single_keys = false\n[actions]\n\"mail.archive\" = \";\"\n");
    assert_eq!(mode_problems, Vec::<String>::new());
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
fn an_action_key_that_clashes_is_left_out() {
    let star = defaults()
        .get("mail.star")
        .expect("mail.star is defined")
        .keys[0]
        .clone();
    let actions = BTreeMap::from([("mail.archive".to_string(), vec![star.clone()])]);
    let remapped = defaults().remapped(&actions);
    assert!(remapped.applied.is_empty());
    assert!(
        remapped
            .problems
            .iter()
            .any(|p| p.starts_with("mail.archive:") && p.contains(&format!("`{star}`"))),
        "{:?}",
        remapped.problems
    );
    assert_eq!(remapped.keymap, *defaults());
}

#[test]
fn a_remap_onto_a_moved_key_drops_that_notice() {
    // Its own definition, since which moves the shipped one announces
    // changes as they expire (D171).
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
        keys = ["x"]
        "#,
    )
    .expect("parses");
    assert!(keymap.moved_entry("updates", "A").is_some());
    let remapped = keymap.with_action_keys("updates.attach", &["A".to_string()]);
    assert!(
        remapped.moved_entry("updates", "A").is_none(),
        "the old notice would hide the new key"
    );
}

#[test]
fn single_character_keys_are_the_ones_the_switch_turns_off() {
    for key in ["e", "E", "#", "?", "g i", "+", "1"] {
        assert!(is_character_key(key), "{key}");
    }
    for key in [
        "Mod+k",
        "Ctrl+d",
        "Enter",
        "Escape",
        "Space",
        "ArrowDown",
        "Shift+Enter",
    ] {
        assert!(!is_character_key(key), "{key}");
    }
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

#[test]
fn action_keys_reach_the_modes_under_their_own_sections() {
    let actions = BTreeMap::from([
        ("todo.done".to_string(), vec!["x".to_string()]),
        ("now.done".to_string(), Vec::new()),
    ]);
    let (sections, _) = remaps("[todo]\n\"y\" = \"todo.edit\"\n");
    let remaps = sections.with_actions(&actions, defaults());
    assert_eq!(remaps.lookup("todo", "x"), Some(Some("todo.done")));
    assert!(remaps.retires("todo", "todo.done"));
    assert_eq!(remaps.lookup("todo", "y"), Some(Some("todo.edit")));
    assert_eq!(remaps.lookup("now", "e"), Some(None));
    // A mode section that moved the action itself wins.
    let (own, _) = remaps_text("[todo]\n\"z\" = \"todo.done\"\n");
    let own = own.with_actions(&actions, defaults());
    assert_eq!(own.lookup("todo", "x"), None);
    assert_eq!(own.key_for("todo", "todo.done"), Some("z"));
}

fn remaps_text(text: &str) -> (ModeRemaps, Vec<String>) {
    remaps(text)
}

#[test]
fn a_tui_screen_key_is_not_remapped_through_actions() {
    let problems = action_keys_problems(defaults(), "tui.rules.new", &["n".to_string()]);
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("TUI screen or overlay")),
        "{problems:?}"
    );
}
