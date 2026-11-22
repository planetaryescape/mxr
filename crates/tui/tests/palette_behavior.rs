use mxr_tui::action::{Action, UiContext};
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
