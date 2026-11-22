use mxr_tui::action::{Action, UiContext};
use mxr_tui::app::{App, Screen};
use mxr_tui::ui::command_palette::{CommandPalette, PaletteCommand};

fn palette_with(commands: Vec<PaletteCommand>) -> CommandPalette {
    let mut palette = CommandPalette::with_commands(commands);
    palette.toggle_with_archive_operations(UiContext::MailboxList, &[]);
    palette
}

fn type_query(palette: &mut CommandPalette, query: &str) {
    for ch in query.chars() {
        palette.on_char(ch);
    }
}

#[test]
fn palette_opens_from_any_screen() {
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
            "palette must open from screen {screen:?}"
        );
    }
}

#[test]
fn confirm_returns_selected_command_action() {
    let mut palette = palette_with(vec![
        PaletteCommand {
            id: None,
            label: "Reply".into(),
            shortcut: "r".into(),
            action: Action::Reply,
            category: "test".into(),
        },
        PaletteCommand {
            id: None,
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
    let mut palette = palette_with(vec![
        PaletteCommand {
            id: None,
            label: "Compose".into(),
            shortcut: "c".into(),
            action: Action::Compose,
            category: "test".into(),
        },
        PaletteCommand {
            id: None,
            label: "Star".into(),
            shortcut: "s".into(),
            action: Action::Star,
            category: "test".into(),
        },
        PaletteCommand {
            id: None,
            label: "Reply".into(),
            shortcut: "r".into(),
            action: Action::Reply,
            category: "test".into(),
        },
    ]);

    type_query(&mut palette, "star");
    let _ = palette.confirm();

    palette.toggle_with_archive_operations(UiContext::MailboxList, &[]);

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
fn prefix_match_ranks_above_substring_match() {
    let mut palette = palette_with(vec![
        PaletteCommand {
            id: None,
            label: "Mark for Reply".into(),
            shortcut: "x".into(),
            action: Action::Noop,
            category: "test".into(),
        },
        PaletteCommand {
            id: None,
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
