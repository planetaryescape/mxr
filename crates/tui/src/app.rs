use crate::action::Action;
use crate::client::Client;
use crate::input::InputHandler;
use crate::ui;
use mxr_core::types::*;
use mxr_core::MxrError;
use ratatui::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    Sidebar,
    MailList,
}

pub struct App {
    pub envelopes: Vec<Envelope>,
    pub labels: Vec<Label>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub active_pane: ActivePane,
    pub should_quit: bool,
    input: InputHandler,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            envelopes: Vec::new(),
            labels: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            active_pane: ActivePane::MailList,
            should_quit: false,
            input: InputHandler::new(),
        }
    }

    pub async fn load(&mut self, client: &mut Client) -> Result<(), MxrError> {
        self.envelopes = client.list_envelopes(100, 0).await?;
        self.labels = client.list_labels().await?;
        Ok(())
    }

    pub fn input_pending(&self) -> bool {
        self.input.is_pending()
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        self.input.handle_key(key)
    }

    pub fn tick(&mut self) {
        self.input.check_timeout();
    }

    pub fn apply(&mut self, action: Action) {
        match action {
            Action::MoveDown => {
                if self.selected_index + 1 < self.envelopes.len() {
                    self.selected_index += 1;
                }
            }
            Action::MoveUp => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            Action::JumpTop => {
                self.selected_index = 0;
                self.scroll_offset = 0;
            }
            Action::JumpBottom => {
                if !self.envelopes.is_empty() {
                    self.selected_index = self.envelopes.len() - 1;
                }
            }
            Action::PageDown => {
                let page = 20;
                self.selected_index =
                    (self.selected_index + page).min(self.envelopes.len().saturating_sub(1));
            }
            Action::PageUp => {
                let page = 20;
                self.selected_index = self.selected_index.saturating_sub(page);
            }
            Action::ViewportTop => {
                self.selected_index = self.scroll_offset;
            }
            Action::ViewportMiddle => {
                let visible_height = 20;
                self.selected_index = (self.scroll_offset + visible_height / 2)
                    .min(self.envelopes.len().saturating_sub(1));
            }
            Action::ViewportBottom => {
                let visible_height = 20;
                self.selected_index = (self.scroll_offset + visible_height)
                    .min(self.envelopes.len().saturating_sub(1));
            }
            Action::CenterCurrent => {
                let visible_height = 20;
                self.scroll_offset = self.selected_index.saturating_sub(visible_height / 2);
            }
            Action::SwitchPane => {
                self.active_pane = match self.active_pane {
                    ActivePane::Sidebar => ActivePane::MailList,
                    ActivePane::MailList => ActivePane::Sidebar,
                };
            }
            Action::OpenSelected => {
                // Phase 0: no-op
            }
            Action::Back => {
                // Phase 0: no-op
            }
            Action::QuitView => {
                self.should_quit = true;
            }
        }
    }

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
            .split(area);

        let sidebar_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(chunks[0]);

        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(chunks[1]);

        ui::sidebar::draw(frame, sidebar_chunks[0], &self.labels, &self.active_pane);
        ui::mail_list::draw(
            frame,
            main_chunks[0],
            &self.envelopes,
            self.selected_index,
            self.scroll_offset,
            &self.active_pane,
        );

        let status_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        ui::status_bar::draw(frame, status_area[1], &self.envelopes);
    }
}
