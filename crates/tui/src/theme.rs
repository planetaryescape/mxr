use ratatui::style::{Color, Modifier, Style};

/// Centralized color palette for the TUI.
/// All UI code should use theme methods instead of hardcoded colors.
pub struct Theme {
    // Borders
    pub border_focused: Color,
    pub border_unfocused: Color,

    // Text
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,

    // Accent / highlight
    pub accent: Color,
    pub accent_dim: Color,

    // Selection
    pub selection_bg: Color,
    pub selection_fg: Color,

    // Semantic
    pub error: Color,
    pub warning: Color,
    pub success: Color,

    // Specific UI
    pub unread_fg: Color,
    pub label_bg: Color,
    pub modal_bg: Color,
    pub hint_bar_bg: Color,
    pub quote_fg: Color,
    pub signature_fg: Color,
    pub line_number_fg: Color,
}

impl Theme {
    /// Dark theme using colors that match the terminal-native aesthetic.
    pub fn dark() -> Self {
        Self {
            border_focused: Color::Cyan,
            border_unfocused: Color::DarkGray,
            text_primary: Color::White,
            text_secondary: Color::Gray,
            text_muted: Color::DarkGray,
            accent: Color::Cyan,
            accent_dim: Color::Blue,
            selection_bg: Color::Rgb(50, 50, 60),
            selection_fg: Color::White,
            error: Color::Red,
            warning: Color::Yellow,
            success: Color::Green,
            unread_fg: Color::White,
            label_bg: Color::Rgb(40, 40, 50),
            modal_bg: Color::Rgb(18, 18, 26),
            hint_bar_bg: Color::Rgb(30, 30, 40),
            quote_fg: Color::DarkGray,
            signature_fg: Color::DarkGray,
            line_number_fg: Color::Rgb(80, 80, 80),
        }
    }

    // Helper style methods
    pub fn border_style(&self, focused: bool) -> Style {
        if focused {
            Style::default().fg(self.border_focused)
        } else {
            Style::default().fg(self.border_unfocused)
        }
    }

    pub fn highlight_style(&self) -> Style {
        Style::default()
            .bg(self.selection_bg)
            .fg(self.selection_fg)
            .add_modifier(Modifier::BOLD)
    }

    pub fn accent_style(&self) -> Style {
        Style::default().fg(self.accent)
    }

    pub fn muted_style(&self) -> Style {
        Style::default().fg(self.text_muted)
    }

    pub fn primary_style(&self) -> Style {
        Style::default().fg(self.text_primary)
    }

    pub fn secondary_style(&self) -> Style {
        Style::default().fg(self.text_secondary)
    }

    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error)
    }

    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning)
    }

    pub fn success_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    pub fn unread_style(&self) -> Style {
        Style::default()
            .fg(self.unread_fg)
            .add_modifier(Modifier::BOLD)
    }

    pub fn modal_block_style(&self) -> Style {
        Style::default().bg(self.modal_bg)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
