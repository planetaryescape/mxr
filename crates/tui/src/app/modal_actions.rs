use super::*;

impl App {
    pub(super) fn apply_modal_action(&mut self, action: Action) {
        match action {
            Action::OpenCommandPalette => {
                self.command_palette.palette.keybindings = self.keybindings.clone();
                self.command_palette.palette.binding_context = Some(self.keybinding_context());
                self.command_palette
                    .palette
                    .toggle(self.current_ui_context());
            }
            Action::CloseCommandPalette => {
                self.command_palette.palette.visible = false;
            }
            // Sync
            Action::Help => {
                self.modals.help_open = !self.modals.help_open;
                self.modals.help_scroll_offset = 0;
                self.modals.help_query.clear();
                self.modals.help_selected = 0;
                self.modals.help_context_filter = None;
            }
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}
