use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

/// Centralized color palette for the TUI.
/// All UI code should use theme methods instead of hardcoded colors.
#[derive(Debug, Clone)]
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
    pub link_fg: Color,
}

impl Theme {
    pub fn from_spec(spec: &str) -> Self {
        let normalized = spec.trim();
        if normalized.is_empty() {
            return Self::default();
        }

        match normalized.to_ascii_lowercase().as_str() {
            "default" | "dark" => Self::dark(),
            "minimal" => Self::minimal(),
            "light" => Self::light(),
            "catppuccin" => Self::catppuccin(),
            _ => Self::from_path(normalized).unwrap_or_default(),
        }
    }

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
            link_fg: Color::Rgb(96, 165, 250), // blue, underlined in render
        }
    }

    pub fn minimal() -> Self {
        Self {
            border_focused: Color::White,
            border_unfocused: Color::DarkGray,
            text_primary: Color::White,
            text_secondary: Color::Gray,
            text_muted: Color::DarkGray,
            accent: Color::White,
            accent_dim: Color::Gray,
            selection_bg: Color::Black,
            selection_fg: Color::White,
            error: Color::Red,
            warning: Color::Yellow,
            success: Color::Green,
            unread_fg: Color::White,
            label_bg: Color::Black,
            modal_bg: Color::Black,
            hint_bar_bg: Color::Black,
            quote_fg: Color::Gray,
            signature_fg: Color::DarkGray,
            line_number_fg: Color::DarkGray,
            link_fg: Color::Cyan,
        }
    }

    pub fn light() -> Self {
        Self {
            border_focused: Color::Blue,
            border_unfocused: Color::Gray,
            text_primary: Color::Black,
            text_secondary: Color::DarkGray,
            text_muted: Color::Gray,
            accent: Color::Blue,
            accent_dim: Color::Cyan,
            selection_bg: Color::Rgb(225, 235, 255),
            selection_fg: Color::Black,
            error: Color::Red,
            warning: Color::Rgb(180, 120, 0),
            success: Color::Green,
            unread_fg: Color::Black,
            label_bg: Color::Rgb(236, 242, 255),
            modal_bg: Color::Rgb(248, 249, 252),
            hint_bar_bg: Color::Rgb(240, 244, 248),
            quote_fg: Color::Gray,
            signature_fg: Color::Gray,
            line_number_fg: Color::Gray,
            link_fg: Color::Blue,
        }
    }

    pub fn catppuccin() -> Self {
        Self {
            border_focused: Color::Rgb(137, 180, 250),
            border_unfocused: Color::Rgb(88, 91, 112),
            text_primary: Color::Rgb(205, 214, 244),
            text_secondary: Color::Rgb(186, 194, 222),
            text_muted: Color::Rgb(127, 132, 156),
            accent: Color::Rgb(203, 166, 247),
            accent_dim: Color::Rgb(137, 180, 250),
            selection_bg: Color::Rgb(49, 50, 68),
            selection_fg: Color::Rgb(205, 214, 244),
            error: Color::Rgb(243, 139, 168),
            warning: Color::Rgb(249, 226, 175),
            success: Color::Rgb(166, 227, 161),
            unread_fg: Color::Rgb(205, 214, 244),
            label_bg: Color::Rgb(69, 71, 90),
            modal_bg: Color::Rgb(30, 30, 46),
            hint_bar_bg: Color::Rgb(49, 50, 68),
            quote_fg: Color::Rgb(108, 112, 134),
            signature_fg: Color::Rgb(127, 132, 156),
            line_number_fg: Color::Rgb(88, 91, 112),
            link_fg: Color::Rgb(137, 180, 250),
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

    /// Returns a color for a label based on its name.
    /// System labels get fixed colors, user labels use a hash-based palette.
    pub fn label_color(label_name: &str) -> Color {
        match label_name.to_uppercase().as_str() {
            "INBOX" => Color::Blue,
            "STARRED" => Color::Yellow,
            "SENT" => Color::Gray,
            "DRAFT" => Color::Magenta,
            "TRASH" => Color::Red,
            "SPAM" => Color::Rgb(255, 140, 0),
            "ARCHIVE" => Color::DarkGray,
            "IMPORTANT" => Color::Yellow,
            _ => {
                // Hash-based color for user labels
                let hash: u8 = label_name.bytes().fold(0u8, |acc, b| acc.wrapping_add(b));
                let colors = [
                    Color::Rgb(96, 165, 250),  // blue
                    Color::Rgb(52, 211, 153),  // emerald
                    Color::Rgb(251, 146, 60),  // orange
                    Color::Rgb(167, 139, 250), // violet
                    Color::Rgb(251, 113, 133), // rose
                    Color::Rgb(56, 189, 248),  // sky
                    Color::Rgb(253, 186, 116), // amber
                    Color::Rgb(134, 239, 172), // green
                ];
                colors[(hash % colors.len() as u8) as usize]
            }
        }
    }

    fn from_path(path: &str) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let overrides = toml::from_str::<ThemeOverrides>(&content).ok()?;
        Some(overrides.apply(Self::dark()))
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ThemeOverrides {
    border_focused: Option<ColorValue>,
    border_unfocused: Option<ColorValue>,
    text_primary: Option<ColorValue>,
    text_secondary: Option<ColorValue>,
    text_muted: Option<ColorValue>,
    accent: Option<ColorValue>,
    accent_dim: Option<ColorValue>,
    selection_bg: Option<ColorValue>,
    selection_fg: Option<ColorValue>,
    error: Option<ColorValue>,
    warning: Option<ColorValue>,
    success: Option<ColorValue>,
    unread_fg: Option<ColorValue>,
    label_bg: Option<ColorValue>,
    modal_bg: Option<ColorValue>,
    hint_bar_bg: Option<ColorValue>,
    quote_fg: Option<ColorValue>,
    signature_fg: Option<ColorValue>,
    line_number_fg: Option<ColorValue>,
    link_fg: Option<ColorValue>,
}

impl ThemeOverrides {
    fn apply(self, mut theme: Theme) -> Theme {
        macro_rules! override_color {
            ($field:ident) => {
                if let Some(value) = self.$field.and_then(ColorValue::into_color) {
                    theme.$field = value;
                }
            };
        }

        override_color!(border_focused);
        override_color!(border_unfocused);
        override_color!(text_primary);
        override_color!(text_secondary);
        override_color!(text_muted);
        override_color!(accent);
        override_color!(accent_dim);
        override_color!(selection_bg);
        override_color!(selection_fg);
        override_color!(error);
        override_color!(warning);
        override_color!(success);
        override_color!(unread_fg);
        override_color!(label_bg);
        override_color!(modal_bg);
        override_color!(hint_bar_bg);
        override_color!(quote_fg);
        override_color!(signature_fg);
        override_color!(line_number_fg);
        override_color!(link_fg);
        theme
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ColorValue {
    Named(String),
    Rgb([u8; 3]),
}

impl ColorValue {
    fn into_color(self) -> Option<Color> {
        match self {
            Self::Rgb([r, g, b]) => Some(Color::Rgb(r, g, b)),
            Self::Named(name) => parse_named_color(&name),
        }
    }
}

fn parse_named_color(value: &str) -> Option<Color> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "dark_gray" | "dark-grey" | "darkgrey" => Some(Color::DarkGray),
        "white" => Some(Color::White),
        _ if normalized.starts_with('#') && normalized.len() == 7 => {
            let r = u8::from_str_radix(&normalized[1..3], 16).ok()?;
            let g = u8::from_str_radix(&normalized[3..5], 16).ok()?;
            let b = u8::from_str_radix(&normalized[5..7], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}
