//! Visual tokens from the approved mockups in `docs/`, as gpui types.
//!
//! Colors live in [`Theme`], which comes in a light and a dark set picked from
//! the window's appearance; sizes and the type scale are appearance-independent
//! consts. Everything the views need should come from here — no literal colors
//! or magic numbers in `popover.rs`, `rings.rs`, `stats.rs`.

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
    /// Secondary text: the plan, ring labels and captions, stat tile labels.
    pub secondary: Rgba,
    /// Tertiary text: the weekday row and the footer.
    pub tertiary: Rgba,
    /// Separator rules, and — the mockup uses the same value — the ring track
    /// and the unfilled part of a model bar.
    pub separator: Rgba,
    /// System blue: ring arcs and the spark bar for today.
    pub accent: Rgba,
    /// Text drawn on top of [`Theme::accent`].
    pub on_accent: Rgba,
    /// System red: a ring whose limit [`is_low`](crate::model::Limit::is_low).
    pub warning: Rgba,
    /// Hover wash on a button or a menu row.
    pub hover: Rgba,
    /// Background of the "···" menu.
    pub menu_bg: Rgba,
    /// The filled part of a per-model bar. Deliberately not the accent: the
    /// mockup keeps blue for limits and draws the model split in ink.
    pub model_bar: Rgba,
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
    hover: hex_a(0x000000, 0.05),
    menu_bg: hex(0xf7f7f9),
    model_bar: hex_a(0x1d1d1f, 0.55),
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
    hover: hex_a(0xffffff, 0.08),
    menu_bg: hex(0x3a3a3c),
    model_bar: hex_a(0xf5f5f7, 0.55),
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

    /// The spark bar colour for a day that is not today: the accent, faded, so
    /// the seven bars read as one series with today picked out of it.
    pub fn spark_past(&self) -> Rgba {
        Rgba {
            a: SPARK_PAST_ALPHA,
            ..self.accent
        }
    }
}

/// Alpha applied to the accent for the six days before today.
pub const SPARK_PAST_ALPHA: f32 = 0.35;

// ── Sizes ────────────────────────────────────────────────────────────────────

/// Popover width. Fixed; height grows with content.
pub const POPOVER_WIDTH: Pixels = px(320.);
/// Corner radius of the popover.
pub const POPOVER_RADIUS: Pixels = px(11.);
/// Gap between the menu bar and the top of the popover.
pub const POPOVER_TOP_GAP: Pixels = px(6.);

/// Horizontal padding for the header, the rules, and every block.
pub const PAD_X: Pixels = px(14.);
/// Size of the header's "···" button.
pub const ICON_BUTTON: Pixels = px(22.);

/// Outer diameter of a limit ring.
pub const RING_SIZE: Pixels = px(56.);
/// Stroke width of both the track and the arc.
pub const RING_STROKE: Pixels = px(6.);
/// Gap between rings in the row.
pub const RING_GAP: Pixels = px(8.);
/// Gap between the ring, its label and its reset caption.
pub const RING_LABEL_GAP: Pixels = px(6.);

/// Height of a per-model bar, as a plain float so the layout constants that add
/// it up stay `const`.
pub const MODEL_BAR_HEIGHT_PX: f32 = 6.0;
/// The same, as gpui's unit.
pub const MODEL_BAR_HEIGHT: Pixels = px(MODEL_BAR_HEIGHT_PX);
/// Corner radius of a per-model bar.
pub const MODEL_BAR_RADIUS: Pixels = px(3.);
/// The tallest a spark bar gets; the busiest day of the week is drawn this tall
/// and the others are scaled against it.
pub const SPARK_MAX_HEIGHT: Pixels = px(28.);
/// Gap between spark bars.
pub const SPARK_GAP: Pixels = px(3.);
/// Corner radius of a spark bar.
pub const SPARK_RADIUS: Pixels = px(2.);

// ── Type scale ───────────────────────────────────────────────────────────────

/// Section titles ("Today", "Last 7 days") and the header.
pub const TEXT_TITLE: Pixels = px(13.);
/// Body text.
pub const TEXT_BODY: Pixels = px(13.);
/// The plan, ring labels, model names.
pub const TEXT_SMALL: Pixels = px(12.);
/// Reset captions, stat tile labels, the footer.
pub const TEXT_TINY: Pixels = px(11.);
/// The weekday row under the spark bars.
pub const TEXT_MICRO: Pixels = px(10.);
/// The number on a stat tile.
pub const TEXT_STAT: Pixels = px(15.);

/// Monospace family, used for every number the popover prints.
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

    #[test]
    fn rings_are_the_mockups_56_by_6() {
        assert_eq!(RING_SIZE, px(56.));
        assert_eq!(RING_STROKE, px(6.));
    }

    #[test]
    fn the_model_bar_is_ink_not_accent() {
        // The mockup's rgba(29,29,31,0.55) light, rgba(245,245,247,0.55) dark.
        assert_eq!((LIGHT.model_bar.r * 255.0).round() as u32, 29);
        assert_eq!((DARK.model_bar.r * 255.0).round() as u32, 245);
        assert!((LIGHT.model_bar.a - 0.55).abs() < 1e-6);
    }

    #[test]
    fn past_spark_bars_are_the_accent_faded() {
        let past = LIGHT.spark_past();
        assert_eq!(past.r, LIGHT.accent.r);
        assert!((past.a - 0.35).abs() < 1e-6);
    }
}
