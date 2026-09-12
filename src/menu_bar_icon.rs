//! The menu bar glyph: a battery-style ring showing how much of the session
//! window is used, drawn at runtime with Core Graphics rather than shipped as
//! an asset, because the percentage changes on every fetch and a template image
//! has to be regenerated to change.
//!
//! Normally the image is marked `isTemplate`, so AppKit throws away the colours
//! and keeps only the alpha — it then tints the ring for the current menu bar
//! appearance (dark, light, and the "reduce transparency" variants) for free,
//! and everything below draws in opaque black on a clear background. The one
//! exception is the low state: when 20% or less of the window is left the ring
//! has to actually be red, so it is drawn in `NSColor::systemRedColor` and the
//! image is *not* a template — a template would tint the red away again.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_app_kit::{NSColor, NSGraphicsContext, NSImage};
use objc2_core_graphics::{CGContext, CGLineCap};
use objc2_foundation::{NSRect, NSSize};

/// Glyph box, in points. Square, and a touch smaller than the menu bar's 22pt
/// height so the ring reads as an icon rather than filling the bar.
pub const SIZE: f64 = 16.0;

/// Ring stroke, in points. Thick enough that the arc is legible at 1x, thin
/// enough that a 1% arc is still a dot on the circle rather than a blob.
pub const STROKE: f64 = 2.5;

/// How visible the unfilled part of the ring is. The track has to read as the
/// "rest of the circle" without competing with the arc.
const TRACK_ALPHA: f64 = 0.3;

/// A menu bar [`NSImage`] of the ring for `percent` of the session window.
///
/// `low` is [`crate::model::Limit::is_low`]: it turns the ring red and takes it
/// out of template mode so the red survives the menu bar's tinting.
pub fn ring_image(percent: f32, low: bool) -> Retained<NSImage> {
    let percent = clamp_percent(percent);
    let handler = RcBlock::new(move |_dirty: NSRect| -> Bool {
        draw(percent, low);
        Bool::YES
    });
    let image =
        NSImage::imageWithSize_flipped_drawingHandler(NSSize::new(SIZE, SIZE), false, &handler);
    image.setTemplate(!low);
    image
}

/// The percentage the ring is actually drawn for. The API takes an `f32` so it
/// can be fed a utilisation straight from the provider, and a server that has
/// gone over its own limit should still draw a full circle, not overdraw.
pub fn clamp_percent(percent: f32) -> f32 {
    if percent.is_nan() {
        0.0
    } else {
        percent.clamp(0.0, 100.0)
    }
}

/// Centre of the ring, in the image's own bottom-left-origin coordinates.
pub fn center() -> (f64, f64) {
    (SIZE / 2.0, SIZE / 2.0)
}

/// Radius of the circle the stroke is centred on: the glyph box inset by half
/// the stroke, so the ring's outer edge lands exactly on the box.
pub fn radius() -> f64 {
    (SIZE - STROKE) / 2.0
}

/// Where the arc starts, in radians: 12 o'clock, the way the system's own
/// progress rings start.
pub fn arc_start_angle() -> f64 {
    std::f64::consts::FRAC_PI_2
}

/// Where the arc for `percent` ends, in radians.
///
/// The image context has a bottom-left origin, so angles increase
/// anticlockwise; sweeping clockwise from 12 o'clock therefore *subtracts* the
/// swept angle. Returns the start angle for 0%, and start minus a full turn for
/// 100%, which Core Graphics draws as the whole circle.
pub fn arc_end_angle(percent: f32) -> f64 {
    arc_start_angle() - arc_sweep(percent)
}

/// How far round the circle `percent` reaches, in radians.
pub fn arc_sweep(percent: f32) -> f64 {
    clamp_percent(percent) as f64 / 100.0 * std::f64::consts::TAU
}

/// Whether there is an arc to draw at all. At 0% only the track shows; drawing
/// a zero-length arc with round caps would put a stray dot at 12 o'clock.
pub fn has_arc(percent: f32) -> bool {
    clamp_percent(percent) > 0.0
}

/// Draw into whatever context AppKit has made current for the drawing handler.
fn draw(percent: f32, low: bool) {
    let Some(ctx) = NSGraphicsContext::currentContext() else {
        return;
    };
    let cg = ctx.CGContext();
    let cg = Some(&*cg);

    let (cx, cy) = center();
    let r = radius();

    CGContext::set_should_antialias(cg, true);
    CGContext::set_line_width(cg, STROKE);
    CGContext::set_line_cap(cg, CGLineCap::Round);

    // The track: the full circle, faint. Butt caps would be visible where a
    // closed circle's ends meet, so the cap style is left as set above and the
    // path is closed by `add_arc` itself.
    set_stroke_colour(cg, low, TRACK_ALPHA);
    CGContext::begin_path(cg);
    CGContext::add_arc(cg, cx, cy, r, 0.0, std::f64::consts::TAU, 0);
    CGContext::stroke_path(cg);

    if !has_arc(percent) {
        return;
    }

    set_stroke_colour(cg, low, 1.0);
    CGContext::begin_path(cg);
    CGContext::add_arc(cg, cx, cy, r, arc_start_angle(), arc_end_angle(percent), 1);
    CGContext::stroke_path(cg);
}

/// The ink. Black for the template image (AppKit tints it), the system red for
/// the low state — taken from `NSColor` rather than hard-coded so it tracks the
/// user's accessibility settings and the menu bar appearance.
fn set_stroke_colour(cg: Option<&CGContext>, low: bool, alpha: f64) {
    if low {
        // `setStroke` writes into the current graphics context, which is the
        // same context `cg` refers to, so the CG path stroking below picks it up.
        NSColor::systemRedColor()
            .colorWithAlphaComponent(alpha)
            .setStroke();
    } else {
        CGContext::set_gray_stroke_color(cg, 0.0, alpha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_sits_inside_the_glyph_box() {
        let (cx, cy) = center();
        assert!(cx - radius() - STROKE / 2.0 >= 0.0);
        assert!(cy + radius() + STROKE / 2.0 <= SIZE);
    }

    #[test]
    fn percentages_are_clamped_to_a_real_ring() {
        assert_eq!(clamp_percent(-5.0), 0.0);
        assert_eq!(clamp_percent(140.0), 100.0);
        assert_eq!(clamp_percent(f32::NAN), 0.0);
        assert_eq!(clamp_percent(42.0), 42.0);
    }

    #[test]
    fn nothing_is_drawn_at_zero_percent() {
        assert!(!has_arc(0.0));
        assert!(!has_arc(-1.0));
        assert!(has_arc(0.5));
    }

    #[test]
    fn the_arc_starts_at_twelve_oclock_and_sweeps_clockwise() {
        // Clockwise in a bottom-left-origin context means a decreasing angle.
        assert_eq!(arc_end_angle(0.0), arc_start_angle());
        assert!(arc_end_angle(25.0) < arc_start_angle());
    }

    #[test]
    fn a_quarter_is_a_quarter_turn_and_a_full_ring_is_a_whole_turn() {
        let quarter = std::f64::consts::FRAC_PI_2;
        assert!((arc_sweep(25.0) - quarter).abs() < 1e-9);
        assert!((arc_sweep(100.0) - std::f64::consts::TAU).abs() < 1e-9);
        assert!((arc_start_angle() - arc_end_angle(100.0) - std::f64::consts::TAU).abs() < 1e-9);
    }

    #[test]
    fn over_a_hundred_percent_is_still_just_a_full_ring() {
        assert_eq!(arc_sweep(250.0), arc_sweep(100.0));
    }
}
