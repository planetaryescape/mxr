use mxr_core::AccountColor;
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

    // Contrast poles for text rendered over computed/tinted backgrounds
    // (e.g. blended selection rows). Pick whichever pole contrasts with
    // the background instead of hardcoding black/white at call sites.
    pub contrast_dark: Color,
    pub contrast_light: Color,

    // Specific UI
    pub unread_fg: Color,
    pub label_bg: Color,
    pub modal_bg: Color,
    pub hint_bar_bg: Color,
    pub quote_fg: Color,
    pub signature_fg: Color,
    pub line_number_fg: Color,
    pub link_fg: Color,

    /// Fills and ink for the account chip (blueprint 33). Only the account
    /// chip reads these, so no other element may take an account colour.
    pub account: AccountPalette,
}

/// Eight account fills in `AccountColor::ALL` order, and the one ink that
/// sits on all of them. Both come from the web's `--account-*` tokens so the
/// two clients show the same colour for the same account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountPalette {
    pub fills: [Color; 8],
    pub ink: Color,
    /// The 256-colour pick for each fill, computed once with the palette.
    picks: [u8; 8],
}

/// Web dark and midnight. Ink is near-black on these fills.
const DARK_FILLS: [Color; 8] = [
    Color::Rgb(0x6c, 0xc0, 0xf0), // sky
    Color::Rgb(0xe8, 0xa3, 0x3d), // amber
    Color::Rgb(0x3f, 0xbf, 0x94), // green
    Color::Rgb(0xe9, 0xdc, 0x5c), // yellow
    Color::Rgb(0x7a, 0xa6, 0xf5), // blue
    Color::Rgb(0xf0, 0x8a, 0x5d), // vermilion
    Color::Rgb(0xd5, 0x8f, 0xc0), // purple
    Color::Rgb(0xa8, 0xb3, 0xbf), // slate
];
const DARK_INK: Color = Color::Rgb(0x0b, 0x12, 0x18);

/// Web light. Fills are deeper so white ink reaches 4.5:1.
const LIGHT_FILLS: [Color; 8] = [
    Color::Rgb(0x1f, 0x6f, 0xa8), // sky
    Color::Rgb(0x8f, 0x5a, 0x00), // amber
    Color::Rgb(0x00, 0x78, 0x5a), // green
    Color::Rgb(0x6f, 0x64, 0x00), // yellow
    Color::Rgb(0x2f, 0x55, 0xb8), // blue
    Color::Rgb(0xb3, 0x47, 0x1b), // vermilion
    Color::Rgb(0x9a, 0x4a, 0x84), // purple
    Color::Rgb(0x56, 0x60, 0x6b), // slate
];
const LIGHT_INK: Color = Color::Rgb(0xff, 0xff, 0xff);

impl AccountPalette {
    pub fn new(fills: [Color; 8], ink: Color) -> Self {
        Self {
            fills,
            ink,
            picks: distinct_256_picks(&fills),
        }
    }

    /// The slot's 256-colour pick. The eight picks are distinct, so a
    /// palette that maps two fills to one index is never shown.
    pub fn indexed_fill(&self, color: AccountColor) -> Color {
        let slot = AccountColor::ALL
            .iter()
            .position(|candidate| *candidate == color)
            .unwrap_or(7);
        Color::Indexed(self.picks[slot])
    }

    pub fn indexed_ink(&self) -> Color {
        Color::Indexed(nearest_256(self.ink))
    }

    pub fn fill(&self, color: AccountColor) -> Color {
        // A slot this build doesn't know (`Unknown`) falls back to slate, the last one.
        let slot = AccountColor::ALL
            .iter()
            .position(|candidate| *candidate == color)
            .unwrap_or(7);
        self.fills[slot]
    }
}

/// The xterm 256-colour index nearest to `color`, from the 6x6x6 cube and
/// the grey ramp (indices 16 to 255). Non-RGB colours map to black.
pub fn nearest_256(color: Color) -> u8 {
    let Color::Rgb(r, g, b) = color else {
        return 16;
    };
    let target = [i32::from(r), i32::from(g), i32::from(b)];
    (16u8..=255)
        .min_by_key(|index| squared_distance(indexed_rgb(*index), target))
        .unwrap_or(16)
}

/// Each fill's nearest pick, taken in slot order: where two fills share a
/// nearest index, the later one takes its next-nearest unused index.
fn distinct_256_picks(fills: &[Color; 8]) -> [u8; 8] {
    let mut used: Vec<u8> = Vec::with_capacity(8);
    let mut picks = [16u8; 8];
    for (slot, fill) in fills.iter().enumerate() {
        let Color::Rgb(r, g, b) = *fill else {
            continue;
        };
        let target = [i32::from(r), i32::from(g), i32::from(b)];
        let mut candidates: Vec<u8> = (16u8..=255).collect();
        candidates.sort_by_key(|index| squared_distance(indexed_rgb(*index), target));
        let pick = candidates
            .into_iter()
            .find(|index| !used.contains(index))
            .unwrap_or(16);
        used.push(pick);
        picks[slot] = pick;
    }
    picks
}

fn indexed_rgb(index: u8) -> [i32; 3] {
    const LEVELS: [i32; 6] = [0, 95, 135, 175, 215, 255];
    if index >= 232 {
        let grey = 8 + 10 * i32::from(index - 232);
        return [grey, grey, grey];
    }
    let cube = usize::from(index - 16);
    [LEVELS[cube / 36], LEVELS[(cube / 6) % 6], LEVELS[cube % 6]]
}

fn squared_distance(a: [i32; 3], b: [i32; 3]) -> i32 {
    a.iter().zip(b).map(|(x, y)| (x - y).pow(2)).sum()
}

impl Theme {
    pub fn from_spec(spec: &str) -> Self {
        let normalized = spec.trim();
        if normalized.is_empty() {
            return Self::default();
        }

        match normalized.to_ascii_lowercase().as_str() {
            "default" | "dark" => Self::dark(),
            "midnight" => Self::midnight(),
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
            contrast_dark: Color::Black,
            contrast_light: Color::White,
            unread_fg: Color::White,
            label_bg: Color::Rgb(40, 40, 50),
            modal_bg: Color::Rgb(18, 18, 26),
            hint_bar_bg: Color::Rgb(30, 30, 40),
            quote_fg: Color::DarkGray,
            signature_fg: Color::DarkGray,
            line_number_fg: Color::Rgb(80, 80, 80),
            link_fg: Color::Rgb(96, 165, 250), // blue, underlined in render
            account: AccountPalette::new(DARK_FILLS, DARK_INK),
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
            contrast_dark: Color::Black,
            contrast_light: Color::White,
            unread_fg: Color::White,
            label_bg: Color::Black,
            modal_bg: Color::Black,
            hint_bar_bg: Color::Black,
            quote_fg: Color::Gray,
            signature_fg: Color::DarkGray,
            line_number_fg: Color::DarkGray,
            link_fg: Color::Cyan,
            account: AccountPalette::new(DARK_FILLS, DARK_INK),
        }
    }

    /// A navy palette with cool blue accents for terminals that support RGB.
    /// The text tiers are the web's `--text-1..3`, the hint bar is `--ink-room`
    /// and modals are `--ink-overlay` (docs/blueprint/27). The cursor line
    /// stays a blue tint, not `--ink-lamp`: the TUI does not paint a desk, and
    /// the lamp is too close to a terminal's own black to show a cursor.
    /// `default` remains the terminal-native ANSI palette for compatibility.
    pub fn midnight() -> Self {
        Self {
            border_focused: Color::Rgb(84, 221, 255),
            border_unfocused: Color::Rgb(72, 88, 105),
            text_primary: Color::Rgb(226, 233, 240),
            text_secondary: Color::Rgb(166, 178, 192),
            text_muted: Color::Rgb(128, 143, 160),
            accent: Color::Rgb(92, 213, 252),
            accent_dim: Color::Rgb(80, 151, 220),
            selection_bg: Color::Rgb(35, 58, 75),
            selection_fg: Color::Rgb(238, 246, 252),
            error: Color::Rgb(253, 138, 140),
            warning: Color::Rgb(248, 202, 101),
            success: Color::Rgb(104, 215, 161),
            contrast_dark: Color::Rgb(3, 24, 38),
            contrast_light: Color::Rgb(238, 246, 252),
            unread_fg: Color::Rgb(238, 246, 252),
            label_bg: Color::Rgb(1, 57, 74),
            modal_bg: Color::Rgb(31, 42, 53),
            hint_bar_bg: Color::Rgb(8, 15, 23),
            quote_fg: Color::Rgb(153, 166, 181),
            signature_fg: Color::Rgb(153, 166, 181),
            line_number_fg: Color::Rgb(114, 129, 144),
            link_fg: Color::Rgb(126, 190, 255),
            account: AccountPalette::new(DARK_FILLS, DARK_INK),
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
            contrast_dark: Color::Black,
            contrast_light: Color::White,
            unread_fg: Color::Black,
            label_bg: Color::Rgb(236, 242, 255),
            modal_bg: Color::Rgb(248, 249, 252),
            hint_bar_bg: Color::Rgb(240, 244, 248),
            quote_fg: Color::Gray,
            signature_fg: Color::Gray,
            line_number_fg: Color::Gray,
            link_fg: Color::Blue,
            account: AccountPalette::new(LIGHT_FILLS, LIGHT_INK),
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
            contrast_dark: Color::Rgb(17, 17, 27),
            contrast_light: Color::Rgb(205, 214, 244),
            unread_fg: Color::Rgb(205, 214, 244),
            label_bg: Color::Rgb(69, 71, 90),
            modal_bg: Color::Rgb(30, 30, 46),
            hint_bar_bg: Color::Rgb(49, 50, 68),
            quote_fg: Color::Rgb(108, 112, 134),
            signature_fg: Color::Rgb(127, 132, 156),
            line_number_fg: Color::Rgb(88, 91, 112),
            link_fg: Color::Rgb(137, 180, 250),
            account: AccountPalette::new(DARK_FILLS, DARK_INK),
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
                let hash: u8 = label_name.bytes().fold(0u8, u8::wrapping_add);
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

#[cfg(test)]
mod tests {
    use super::{nearest_256, Theme};
    use mxr_core::AccountColor;
    use ratatui::style::Color;

    fn all_distinct<T: PartialEq + std::fmt::Debug>(values: &[T]) -> bool {
        values
            .iter()
            .enumerate()
            .all(|(i, value)| values[i + 1..].iter().all(|other| other != value))
    }

    #[test]
    fn account_fills_stay_distinct_in_truecolor_and_256_colours() {
        for palette in [Theme::dark().account, Theme::light().account] {
            let fills: Vec<Color> = AccountColor::ALL
                .iter()
                .map(|color| palette.fill(*color))
                .collect();
            assert!(all_distinct(&fills), "truecolor fills collide: {fills:?}");

            let picks: Vec<Color> = AccountColor::ALL
                .iter()
                .map(|color| palette.indexed_fill(*color))
                .collect();
            assert!(all_distinct(&picks), "256-colour picks collide: {picks:?}");
        }
    }

    #[test]
    fn nearest_256_maps_cube_and_grey_colours_exactly() {
        assert_eq!(nearest_256(Color::Rgb(0, 0, 0)), 16);
        assert_eq!(nearest_256(Color::Rgb(255, 255, 255)), 231);
        assert_eq!(nearest_256(Color::Rgb(128, 128, 128)), 244);
        assert_eq!(nearest_256(Color::Rgb(255, 0, 0)), 196);
    }

    #[test]
    fn account_ink_is_black_on_dark_and_white_on_light() {
        assert_eq!(Theme::dark().account.indexed_ink(), Color::Indexed(233));
        assert_eq!(Theme::light().account.indexed_ink(), Color::Indexed(231));
    }

    #[test]
    fn midnight_is_selectable_and_default_remains_terminal_native() {
        assert_eq!(
            Theme::from_spec("midnight").accent,
            Color::Rgb(92, 213, 252)
        );
        assert_eq!(Theme::from_spec("default").accent, Color::Cyan);
        assert_eq!(Theme::default().accent, Color::Cyan);
    }
}
