//! Visual tokens from the approved mockup in `docs/`, as gpui types.
//!
//! Colors live in [`Theme`], which comes in a light and a dark set picked from
//! the window's appearance; sizes and the type scale are appearance-independent
//! consts. Everything the views need should come from here — no literal colors
//! or magic numbers in `popover.rs`, `stats.rs`.
//!
//! The popover is deliberately monochrome, the way the system Battery menu is:
//! ink on a near-white material, with [`Theme::warning`] the only colour in
//! the whole surface and only when a limit is nearly spent.

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
    /// Popover background — the material the rows sit on.
    pub bg: Rgba,
    /// Hairline border around the popover.
    pub border: Rgba,
    /// Primary text, and — the popover has no accent — the fill of every bar.
    pub text: Rgba,
    /// Secondary text: percentages, reset captions, the values on the Today
    /// rows.
    pub secondary: Rgba,
    /// Tertiary text: the "Menu bar shows" section label and the notes under
    /// the login row.
    pub tertiary: Rgba,
    /// Separator rules, and — the mockup uses the same value — the track a
    /// limit bar is drawn in.
    pub separator: Rgba,
    /// System red: a limit that [`is_low`](crate::model::Limit::is_low). The
    /// only colour in the popover.
    pub warning: Rgba,
    /// Hover wash on a menu row.
    pub hover: Rgba,
}

/// Light appearance (the mockup's own palette).
pub const LIGHT: Theme = Theme {
    // Sampled off the Battery menu on a live screen: its material lands at
    // about (238, 237, 239) over a light page, which this grey at
    // `BG_ALPHA` reproduces. The plain white of the first mockup read as a
    // card, not a menu.
    bg: Rgba {
        r: 236.0 / 255.0,
        g: 236.0 / 255.0,
        b: 238.0 / 255.0,
        a: BG_ALPHA,
    },
    // The system menu has no visible hairline of its own, only the shadow's
    // edge; a faint rim is all that keeps the corners crisp against a busy
    // desktop.
    border: hex_a(0x000000, 0.06),
    // System label colour: black at 85%, so text sits in the material rather
    // than on top of it.
    text: hex_a(0x000000, 0.85),
    secondary: hex(0x6e6e73),
    tertiary: hex(0xaeaeb2),
    separator: hex_a(0x000000, 0.09),
    warning: hex(0xd70015),
    hover: hex_a(0x000000, 0.06),
};

/// Dark appearance: the same roles against a dark material.
pub const DARK: Theme = Theme {
    bg: Rgba {
        r: 40.0 / 255.0,
        g: 40.0 / 255.0,
        b: 42.0 / 255.0,
        a: BG_ALPHA,
    },
    border: hex_a(0xffffff, 0.10),
    text: hex_a(0xffffff, 0.85),
    secondary: hex(0x98989d),
    tertiary: hex(0x8e8e93),
    separator: hex_a(0xffffff, 0.12),
    warning: hex(0xff453a),
    hover: hex_a(0xffffff, 0.10),
};

/// How opaque the popover material is. Over a
/// [`Blurred`](gpui::WindowBackgroundAppearance::Blurred) window this is what
/// gives the popover the translucent look of a system menu: what is behind it
/// shows through as a soft wash, and the text stays fully legible. Checked on
/// a live screen against the Battery menu; `1.0` is the opaque fallback.
pub const BG_ALPHA: f32 = 0.85;

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

    /// The spark bar colour for a day that is not today: the ink, faded far
    /// enough back that today reads as the one bar being pointed at.
    pub fn spark_past(&self) -> Rgba {
        Rgba {
            a: SPARK_PAST_ALPHA,
            ..self.text
        }
    }

    /// The fill of a limit bar: ink, or red once the window is nearly spent.
    pub fn limit_bar(&self, is_low: bool) -> Rgba {
        if is_low {
            self.warning
        } else {
            self.text
        }
    }
}

/// Alpha applied to the ink for the six days before today.
pub const SPARK_PAST_ALPHA: f32 = 0.18;

// ── Sizes ────────────────────────────────────────────────────────────────────

/// Popover width. Fixed; height grows with content.
pub const POPOVER_WIDTH: Pixels = px(260.);
/// Corner radius of the popover.
pub const POPOVER_RADIUS: Pixels = px(10.);
/// Gap between the menu bar and the top of the popover. Zero: the system
/// menus hang straight off the bar's bottom edge, and a gap reads as a
/// floating window rather than a menu.
pub const POPOVER_TOP_GAP: Pixels = px(0.);
/// The menu inset: the padding between the popover's edge and its rows, as a
/// plain float so the layout constants that add it up stay `const`.
pub const POPOVER_PAD_PX: f32 = 5.0;
/// The same, as gpui's unit.
pub const POPOVER_PAD: Pixels = px(POPOVER_PAD_PX);

/// Horizontal padding inside a row — the text inset every line shares.
pub const ROW_PAD_X: Pixels = px(10.);
/// Vertical padding inside a row.
pub const ROW_PAD_Y_PX: f32 = 3.0;
pub const ROW_PAD_Y: Pixels = px(ROW_PAD_Y_PX);
/// Corner radius of a row's hover wash.
pub const ROW_RADIUS: Pixels = px(6.);
/// How far a settings row is indented under its disclosure row.
pub const ROW_INDENT: Pixels = px(12.);

/// A separator is inset from the popover's edges the way a menu's is.
pub const SEPARATOR_INSET: Pixels = px(10.);
/// The air above and below a separator.
pub const SEPARATOR_MARGIN_PX: f32 = 5.0;
pub const SEPARATOR_MARGIN: Pixels = px(SEPARATOR_MARGIN_PX);
/// A hairline.
pub const HAIRLINE_PX: f32 = 1.0;
pub const HAIRLINE: Pixels = px(HAIRLINE_PX);

/// Height of a limit's progress bar.
pub const LIMIT_BAR_HEIGHT_PX: f32 = 4.0;
pub const LIMIT_BAR_HEIGHT: Pixels = px(LIMIT_BAR_HEIGHT_PX);
/// Corner radius of a limit bar.
pub const LIMIT_BAR_RADIUS: Pixels = px(2.);
/// The gap between a limit's label row, its bar and its reset caption.
pub const LIMIT_GAP_PX: f32 = 4.0;
pub const LIMIT_GAP: Pixels = px(LIMIT_GAP_PX);
/// The gap between one limit block and the next.
pub const LIMIT_BLOCK_GAP_PX: f32 = 8.0;
pub const LIMIT_BLOCK_GAP: Pixels = px(LIMIT_BLOCK_GAP_PX);

/// The tallest a spark bar gets; the busiest day of the week is drawn this
/// tall and the others are scaled against it.
pub const SPARK_MAX_HEIGHT_PX: f32 = 16.0;
pub const SPARK_MAX_HEIGHT: Pixels = px(SPARK_MAX_HEIGHT_PX);
/// Width of one spark bar, and the gap between two.
pub const SPARK_BAR_WIDTH: Pixels = px(8.);
pub const SPARK_GAP: Pixels = px(3.);
/// Corner radius of a spark bar.
pub const SPARK_RADIUS: Pixels = px(1.5);

// ── Type scale ───────────────────────────────────────────────────────────────

/// Section headers ("Claude Usage", "Today") and every menu row.
pub const TEXT_TITLE: Pixels = px(13.);
/// Body text.
pub const TEXT_BODY: Pixels = px(13.);
/// The signed-out / loading line.
pub const TEXT_SMALL: Pixels = px(12.);
/// Reset captions and the notice line.
pub const TEXT_TINY: Pixels = px(11.);
/// The "Menu bar shows" label and the notes under the login row.
pub const TEXT_MICRO: Pixels = px(10.);

/// Line box of a 13px row. gpui does not lay text out to a round number, so
/// every row states its line height and the height arithmetic uses these.
pub const LINE_TITLE_PX: f32 = 17.0;
pub const LINE_TITLE: Pixels = px(LINE_TITLE_PX);
/// Line box of the 12px signed-out line.
pub const LINE_SMALL_PX: f32 = 16.0;
pub const LINE_SMALL: Pixels = px(LINE_SMALL_PX);
/// Line box of an 11px caption.
pub const LINE_TINY_PX: f32 = 14.0;
pub const LINE_TINY: Pixels = px(LINE_TINY_PX);
/// Line box of a 10px label.
pub const LINE_MICRO_PX: f32 = 13.0;
pub const LINE_MICRO: Pixels = px(LINE_MICRO_PX);

/// Monospace family, used for the tabular numbers on the Today rows.
pub const MONO_FAMILY: &str = "SF Mono";
/// UI family.
pub const UI_FAMILY: &str = ".SystemUIFont";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_background_is_the_system_menu_material() {
        assert_eq!((LIGHT.bg.r * 255.0).round() as u32, 236);
        assert_eq!((DARK.bg.b * 255.0).round() as u32, 42);
        // Translucent over a blurred window: see BG_ALPHA.
        assert!((LIGHT.bg.a - BG_ALPHA).abs() < 1e-6);
        assert!((DARK.bg.a - BG_ALPHA).abs() < 1e-6);
    }

    #[test]
    fn appearance_picks_the_matching_set() {
        assert_eq!(Theme::for_appearance(WindowAppearance::Light), LIGHT);
        assert_eq!(Theme::for_appearance(WindowAppearance::VibrantLight), LIGHT);
        assert_eq!(Theme::for_appearance(WindowAppearance::Dark), DARK);
        assert_eq!(Theme::for_appearance(WindowAppearance::VibrantDark), DARK);
    }

    #[test]
    fn the_popover_is_260_wide() {
        assert_eq!(POPOVER_WIDTH, px(260.));
        assert_eq!(POPOVER_RADIUS, px(10.));
        assert_eq!(POPOVER_PAD, px(5.));
    }

    #[test]
    fn limit_bars_are_the_mockups_four_pixels() {
        assert_eq!(LIMIT_BAR_HEIGHT, px(4.));
        assert_eq!(SPARK_BAR_WIDTH, px(8.));
        assert_eq!(SPARK_MAX_HEIGHT, px(16.));
    }

    /// Red is the only colour the popover ever draws, and only for a limit
    /// that is nearly spent.
    #[test]
    fn a_limit_bar_is_ink_until_it_is_low() {
        assert_eq!(LIGHT.limit_bar(false), LIGHT.text);
        assert_eq!(LIGHT.limit_bar(true), LIGHT.warning);
        assert_eq!(DARK.limit_bar(true), DARK.warning);
    }

    #[test]
    fn past_spark_bars_are_the_ink_faded() {
        let past = LIGHT.spark_past();
        assert_eq!(past.r, LIGHT.text.r);
        assert!((past.a - 0.18).abs() < 1e-6);
    }
}
