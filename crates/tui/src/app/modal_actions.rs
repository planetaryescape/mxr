use super::*;

impl App {
    pub(super) fn apply_modal_action(&mut self, action: Action) {
        match action {
            Action::OpenCommandPalette => {
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
            }
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}
