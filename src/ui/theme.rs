//! Visual tokens from the approved mockups in `docs/`, as gpui types.
//!
//! Colors live in [`Theme`], which comes in a light and a dark set picked from
//! the window's appearance; sizes and the type scale are appearance-independent
//! consts. Everything the views need should come from here — no literal colors
//! or magic numbers in `popover.rs`, `month_grid.rs`, `day_list.rs`,
//! `week_list.rs`.

use gpui::{px, Pixels, Rgba, WindowAppearance};

/// `const`-friendly hex -> [`Rgba`] (gpui's own `rgb()` is not `const`).
const fn hex(value: u32) -> Rgba {
    Rgba {
        r: ((value >> 16) & 0xff) as f32 / 255.0,
        g: ((value >> 8) & 0xff) as f32 / 255.0,
        b: (value & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

/// Same, with an explicit alpha in 0.0..=1.0.
const fn hex_a(value: u32, alpha: f32) -> Rgba {
    let c = hex(value);
    Rgba { a: alpha, ..c }
}

// ── Colors ───────────────────────────────────────────────────────────────────

/// The appearance-dependent half of the tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// Popover background (fully opaque).
    pub bg: Rgba,
    /// Hairline border around the popover.
    pub border: Rgba,
    /// Primary text.
    pub text: Rgba,
    /// Secondary text: times, locations, chevron buttons.
    pub secondary: Rgba,
    /// Tertiary text: weekday letters, week numbers, footer.
    pub tertiary: Rgba,
    /// Separator rules.
    pub separator: Rgba,
    /// System blue: today's circle, the Join button.
    pub accent: Rgba,
    /// Text drawn on top of [`Theme::accent`].
    pub on_accent: Rgba,
    /// System red: the inline "Couldn't archive" note on a reverted row.
    pub warning: Rgba,
    /// Selected (non-today) day circle.
    pub selected_fill: Rgba,
    /// Hover wash on a day cell or a button.
    pub hover: Rgba,
    /// Day numbers outside the visible month.
    pub out_of_month: Rgba,
    /// Weekend day numbers.
    pub weekend: Rgba,
    /// Event dot under an in-month day.
    pub dot: Rgba,
    /// Event dot under an out-of-month or weekend day.
    pub dot_dim: Rgba,
    /// Background of the expanded event row.
    pub expanded_bg: Rgba,
    /// The row disclosure chevron.
    pub chevron: Rgba,
    /// Secondary button fill ("Open in Google Calendar").
    pub button_bg: Rgba,
    /// Background of the "···" menu.
    pub menu_bg: Rgba,
    /// Fallback color bar when an event has no calendar color.
    pub event_bar: Rgba,
    /// The "–" placeholder in the week list.
    pub placeholder: Rgba,
    /// Raised card inside the popover (the triage body).
    pub card: Rgba,
}

/// Light appearance (the mockup's own palette).
pub const LIGHT: Theme = Theme {
    bg: Rgba {
        r: 240.0 / 255.0,
        g: 240.0 / 255.0,
        b: 242.0 / 255.0,
        a: 1.0,
    },
    border: hex_a(0x000000, 0.12),
    text: hex(0x1d1d1f),
    secondary: hex(0x6e6e73),
    tertiary: hex(0xaeaeb2),
    separator: hex_a(0x000000, 0.08),
    accent: hex(0x0a7aff),
    on_accent: hex(0xffffff),
    warning: hex(0xd70015),
    selected_fill: hex_a(0x000000, 0.08),
    hover: hex_a(0x000000, 0.05),
    out_of_month: hex(0xc7c7cc),
    weekend: hex(0xaeaeb2),
    dot: hex(0xaeaeb2),
    dot_dim: hex(0xd1d1d6),
    expanded_bg: hex_a(0x000000, 0.045),
    chevron: hex(0xc7c7cc),
    button_bg: hex_a(0x000000, 0.06),
    menu_bg: hex(0xf7f7f9),
    event_bar: hex(0x8e8e93),
    placeholder: hex(0xc7c7cc),
    card: hex_a(0xffffff, 0.70),
};

/// Dark appearance: the same roles against a dark material.
pub const DARK: Theme = Theme {
    bg: Rgba {
        r: 40.0 / 255.0,
        g: 40.0 / 255.0,
        b: 42.0 / 255.0,
        a: 1.0,
    },
    border: hex_a(0xffffff, 0.14),
    text: hex(0xf5f5f7),
    secondary: hex(0x98989d),
    tertiary: hex(0x8e8e93),
    separator: hex_a(0xffffff, 0.10),
    accent: hex(0x0a84ff),
    on_accent: hex(0xffffff),
    warning: hex(0xff453a),
    selected_fill: hex_a(0xffffff, 0.14),
    hover: hex_a(0xffffff, 0.08),
    out_of_month: hex(0x545458),
    weekend: hex(0x8e8e93),
    dot: hex(0x8e8e93),
    dot_dim: hex(0x545458),
    expanded_bg: hex_a(0xffffff, 0.07),
    chevron: hex(0x636366),
    button_bg: hex_a(0xffffff, 0.12),
    menu_bg: hex(0x3a3a3c),
    event_bar: hex(0x8e8e93),
    placeholder: hex(0x636366),
    card: hex_a(0xffffff, 0.06),
};

impl Default for Theme {
    fn default() -> Self {
        LIGHT
    }
}

impl Theme {
    /// Pick the set matching the window's appearance.
    pub fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => DARK,
            WindowAppearance::Light | WindowAppearance::VibrantLight => LIGHT,
        }
    }
}

/// Opacity applied to events that have already ended.
pub const PAST_OPACITY: f32 = 0.45;

// ── Sizes ────────────────────────────────────────────────────────────────────

/// Popover width. Fixed; height grows with content.
pub const POPOVER_WIDTH: Pixels = px(320.);
/// Corner radius of the popover.
pub const POPOVER_RADIUS: Pixels = px(11.);
/// Gap between the menu bar and the top of the popover.
pub const POPOVER_TOP_GAP: Pixels = px(6.);

/// Horizontal padding for the header, rule, and footer.
pub const PAD_X: Pixels = px(14.);
/// Horizontal padding for the grid.
pub const GRID_PAD_X: Pixels = px(10.);
/// Horizontal padding for the event list.
pub const LIST_PAD_X: Pixels = px(6.);

/// Height of one day cell in the month grid.
pub const CELL_HEIGHT: Pixels = px(32.);
/// Diameter of the today / selected circle inside a cell.
pub const CELL_CIRCLE: Pixels = px(26.);
/// Width of the ISO week-number gutter on the left of the grid.
pub const WEEK_COLUMN_WIDTH: Pixels = px(22.);
/// Size of the event dot under a day number.
pub const DOT_SIZE: Pixels = px(3.);
/// Diameter of the blue unread dot on a thread row.
pub const UNREAD_DOT: Pixels = px(6.);

/// Width of the color bar on an event row.
pub const BAR_WIDTH: Pixels = px(3.);
/// Height of the color bar on a collapsed event row.
pub const BAR_HEIGHT: Pixels = px(26.);
/// Corner radius of an event row.
pub const ROW_RADIUS: Pixels = px(7.);
/// Size of the header's chevron / "···" buttons.
pub const ICON_BUTTON: Pixels = px(22.);
/// Width of the DOW/day column in the week list.
pub const WEEK_LABEL_WIDTH: Pixels = px(40.);

// ── Type scale ───────────────────────────────────────────────────────────────

/// Month title and event titles.
pub const TEXT_TITLE: Pixels = px(13.);
/// Body: event titles, expanded detail lines.
pub const TEXT_BODY: Pixels = px(13.);
/// Secondary: day numbers, detail lines, buttons.
pub const TEXT_SMALL: Pixels = px(12.);
/// Event times, footer.
pub const TEXT_TINY: Pixels = px(11.);
/// Weekday letters and week numbers.
pub const TEXT_MICRO: Pixels = px(10.);

/// Monospace family, used sparingly.
pub const MONO_FAMILY: &str = "SF Mono";
/// UI family.
pub const UI_FAMILY: &str = ".SystemUIFont";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_background_is_the_mockup_material() {
        assert_eq!((LIGHT.bg.r * 255.0).round() as u32, 240);
        assert_eq!((LIGHT.bg.b * 255.0).round() as u32, 242);
        assert!((LIGHT.bg.a - 1.0).abs() < 1e-6);
    }

    #[test]
    fn accent_is_system_blue() {
        assert_eq!((LIGHT.accent.r * 255.0).round() as u32, 0x0a);
        assert_eq!((LIGHT.accent.g * 255.0).round() as u32, 0x7a);
        assert_eq!((LIGHT.accent.b * 255.0).round() as u32, 0xff);
    }

    #[test]
    fn appearance_picks_the_matching_set() {
        assert_eq!(Theme::for_appearance(WindowAppearance::Light), LIGHT);
        assert_eq!(Theme::for_appearance(WindowAppearance::VibrantLight), LIGHT);
        assert_eq!(Theme::for_appearance(WindowAppearance::Dark), DARK);
        assert_eq!(Theme::for_appearance(WindowAppearance::VibrantDark), DARK);
    }

    #[test]
    fn popover_is_320_wide() {
        assert_eq!(POPOVER_WIDTH, px(320.));
        assert_eq!(POPOVER_RADIUS, px(11.));
    }
}
