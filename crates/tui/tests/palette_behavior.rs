//! Behavior tests for the command palette.
//!
//! Tests the discovery surface power users rely on: ranking, dispatch,
//! recent-command memory. The palette is constructed directly (it has no
//! daemon dependency), so these tests stay tight and fast.

use mxr_tui::action::{Action, UiContext};
use mxr_tui::app::{App, Screen};
use mxr_tui::ui::command_palette::{CommandPalette, PaletteCommand};

fn palette_with(commands: Vec<PaletteCommand>) -> CommandPalette {
    let mut palette = CommandPalette {
        visible: false,
        input: String::new(),
        commands,
        filtered: Vec::new(),
        selected: 0,
        context: UiContext::MailboxList,
        recent_actions: Vec::new(),
    };
    palette.toggle(UiContext::MailboxList);
    palette
}

fn type_query(palette: &mut CommandPalette, query: &str) {
    for ch in query.chars() {
        palette.on_char(ch);
    }
}

#[test]
fn palette_opens_from_any_screen() {
    // The palette is the discovery surface for every action — no matter
    // which screen the user is on. If it only opens on the inbox, power
    // users learn to invoke it inconsistently or stop using it.
    for screen in [
        Screen::Mailbox,
        Screen::Search,
        Screen::Rules,
        Screen::Diagnostics,
        Screen::Accounts,
        Screen::Analytics,
    ] {
        let mut app = App::new();
        app.screen = screen;
        assert!(
            !app.command_palette.palette.visible,
            "precondition: palette closed before activation"
        );

        app.apply(Action::OpenCommandPalette);

        assert!(
            app.command_palette.palette.visible,
            "palette must open from screen {:?}",
            screen
        );
    }
}

#[test]
fn confirm_returns_selected_command_action() {
    // The palette is the dispatcher: a query plus Enter must yield the
    // matching command's Action. Without that, no keyboard discovery
    // path can drive a mutation.
    let mut palette = palette_with(vec![
        PaletteCommand {
            label: "Reply".into(),
            shortcut: "r".into(),
            action: Action::Reply,
            category: "test".into(),
        },
        PaletteCommand {
            label: "Star".into(),
            shortcut: "s".into(),
            action: Action::Star,
            category: "test".into(),
        },
    ]);

    type_query(&mut palette, "sta");
    let action = palette
        .confirm()
        .expect("Enter on a matching command must yield its Action");

    assert_eq!(action, Action::Star);
    assert!(
        !palette.visible,
        "confirming closes the palette so the user returns to context"
    );
}

#[test]
fn recently_used_commands_surface_to_top_with_empty_query() {
    // Power users return to the same handful of commands constantly
    // (compose, reply, archive, star). The palette should learn that
    // pattern and surface recently-used commands at the top of the
    // empty-query list, ahead of registration order.
    let mut palette = palette_with(vec![
        PaletteCommand {
            label: "Compose".into(),
            shortcut: "c".into(),
            action: Action::Compose,
            category: "test".into(),
        },
        PaletteCommand {
            label: "Star".into(),
            shortcut: "s".into(),
            action: Action::Star,
            category: "test".into(),
        },
        PaletteCommand {
            label: "Reply".into(),
            shortcut: "r".into(),
            action: Action::Reply,
            category: "test".into(),
        },
    ]);

    // Use Star — confirming it should record it as recently used.
    type_query(&mut palette, "star");
    let _ = palette.confirm();

    // Reopen the palette with no query.
    palette.toggle(UiContext::MailboxList);

    let labels: Vec<_> = palette
        .visible_commands()
        .map(|cmd| cmd.label.clone())
        .collect();

    assert_eq!(
        labels.first().map(String::as_str),
        Some("Star"),
        "recently confirmed command surfaces above registration-order commands"
    );
}

#[test]
fn most_recent_command_ranks_first_when_multiple_are_recent() {
    // When several commands have been used, the most recent ranks
    // highest. Older recent uses still rank above never-used commands
    // but below the most recent.
    let mut palette = palette_with(vec![
        PaletteCommand {
            label: "Compose".into(),
            shortcut: "c".into(),
            action: Action::Compose,
            category: "test".into(),
        },
        PaletteCommand {
            label: "Star".into(),
            shortcut: "s".into(),
            action: Action::Star,
            category: "test".into(),
        },
        PaletteCommand {
            label: "Reply".into(),
            shortcut: "r".into(),
            action: Action::Reply,
            category: "test".into(),
        },
    ]);

    // Use Star, then Compose. Compose was used most recently.
    type_query(&mut palette, "star");
    let _ = palette.confirm();
    palette.toggle(UiContext::MailboxList);
    type_query(&mut palette, "compose");
    let _ = palette.confirm();
    palette.toggle(UiContext::MailboxList);

    let labels: Vec<_> = palette
        .visible_commands()
        .map(|cmd| cmd.label.clone())
        .collect();

    assert_eq!(
        labels[0], "Compose",
        "most recently used command ranks first"
    );
    assert_eq!(
        labels[1], "Star",
        "earlier recent command ranks above unused commands"
    );
    assert_eq!(
        labels[2], "Reply",
        "unused commands fall to the bottom in registration order"
    );
}

#[test]
fn empty_query_lists_all_commands_in_registration_order() {
    // A freshly opened palette with no query lists every command in the
    // order it was registered. This is the discovery experience for new
    // users who don't yet know what to search for.
    let palette = palette_with(vec![
        PaletteCommand {
            label: "First".into(),
            shortcut: "1".into(),
            action: Action::Noop,
            category: "test".into(),
        },
        PaletteCommand {
            label: "Second".into(),
            shortcut: "2".into(),
            action: Action::Noop,
            category: "test".into(),
        },
        PaletteCommand {
            label: "Third".into(),
            shortcut: "3".into(),
            action: Action::Noop,
            category: "test".into(),
        },
    ]);

    let labels: Vec<_> = palette
        .visible_commands()
        .map(|cmd| cmd.label.clone())
        .collect();

    assert_eq!(
        labels,
        vec![
            "First".to_string(),
            "Second".to_string(),
            "Third".to_string()
        ],
        "empty query preserves registration order"
    );
}

#[test]
fn prefix_match_ranks_above_substring_match() {
    // The user types "rep" expecting "Reply" — not "Mark for Reply" —
    // even though the latter is registered earlier. A user-friendly
    // palette ranks prefix matches above mid-word substring matches.
    let mut palette = palette_with(vec![
        PaletteCommand {
            label: "Mark for Reply".into(),
            shortcut: "x".into(),
            action: Action::Noop,
            category: "test".into(),
        },
        PaletteCommand {
            label: "Reply".into(),
            shortcut: "r".into(),
            action: Action::Reply,
            category: "test".into(),
        },
    ]);

    type_query(&mut palette, "rep");

    let first = palette
        .visible_commands()
        .next()
        .expect("at least one command should match 'rep'");
    assert_eq!(
        first.label, "Reply",
        "prefix match ('Reply') must rank above substring match ('Mark for Reply')"
    );
}
