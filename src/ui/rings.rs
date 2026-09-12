//! The row of limit rings — section 3 of `docs/mockup-popover.dc.html`.
//!
//! One 56px ring per entry in `usage.limits` (session, week, then any
//! model-scoped limits), sharing the width through `flex_grow`. Each ring is a
//! grey track circle with the used fraction drawn over it from twelve o'clock
//! clockwise, the percent centred inside, and the label and reset caption
//! underneath.
//!
//! The rings are geometry, not an asset: they are drawn with [`gpui::canvas`]
//! and [`PathBuilder`], so they follow the theme and stay crisp at any scale
//! factor without shipping SVGs.

use gpui::{
    canvas, div, point, px, Background, Bounds, Context, FontWeight, IntoElement, ParentElement,
    Pixels, Rgba, Styled,
};
use gpui::{Path, PathBuilder};

use crate::model::Limit;
use crate::ui::popover::Popover;
use crate::ui::theme::{self, Theme};

// ── Metrics, kept next to the layout they describe ───────────────────────────

/// Padding above and below the ring row (the mockup's `10px 14px 14px`).
pub const ROW_PAD_TOP: f32 = 10.0;
pub const ROW_PAD_BOTTOM: f32 = 14.0;
/// The 12px semibold label line under a ring. gpui lays text out at roughly
/// 1.6x the font size, which is what every line constant here is.
const LABEL_LINE: f32 = 19.0;
/// The 11px secondary reset caption under the label.
const CAPTION_LINE: f32 = 18.0;
/// The gaps between ring, label and caption.
const LABEL_GAP: f32 = 6.0;

/// What stands in for the rings before the first fetch lands.
const LOADING_LINE: &str = "Checking your limits\u{2026}";

/// A percentage at or below this draws the track only: an arc that short is a
/// dot of colour that reads as a rendering artefact rather than as data.
const MIN_VISIBLE_PERCENT: f32 = 0.5;

/// The row of rings for the current snapshot.
pub fn render(popover: &Popover, _cx: &mut Context<Popover>) -> impl IntoElement {
    let theme = popover.theme();
    let now = popover.now();
    let limits = popover
        .provider()
        .usage()
        .map(|u| u.limits)
        .unwrap_or_default();

    let row = div()
        .flex()
        .flex_row()
        .px(theme::PAD_X)
        .pt(px(ROW_PAD_TOP))
        .pb(px(ROW_PAD_BOTTOM));

    // The first fetch is still out (or came back with nothing usable). The row
    // keeps its height either way, so the popover does not jump when the rings
    // arrive; a word in the middle of it is better than a blank band.
    if limits.is_empty() {
        return row
            // The height of the ring stack alone: the row's own padding is
            // outside it, so this keeps the band exactly as tall as `height()`.
            .h(px(height() - ROW_PAD_TOP - ROW_PAD_BOTTOM))
            .items_center()
            .justify_center()
            .text_size(theme::TEXT_SMALL)
            .text_color(theme.secondary)
            .child(LOADING_LINE);
    }

    row.gap(theme::RING_GAP).children(
        limits
            .iter()
            .map(|limit| ring(limit, theme, now))
            .collect::<Vec<_>>(),
    )
}

/// Height of the ring row. Constant: every ring is the same height, and the
/// row never wraps because the rings share the width.
pub fn height() -> f32 {
    ROW_PAD_TOP
        + f32::from(theme::RING_SIZE)
        + LABEL_GAP
        + LABEL_LINE
        + LABEL_GAP
        + CAPTION_LINE
        + ROW_PAD_BOTTOM
}

/// One ring: the dial, the label, and the reset caption.
fn ring(limit: &Limit, theme: Theme, now: chrono::DateTime<chrono::Local>) -> impl IntoElement {
    let arc_color = if limit.is_low() {
        theme.warning
    } else {
        theme.accent
    };
    let caption = limit.reset_caption(now).unwrap_or_default();

    div()
        .flex()
        .flex_col()
        .flex_grow()
        .items_center()
        .gap(theme::RING_LABEL_GAP)
        .child(dial(limit.percent, theme.separator, arc_color, limit))
        .child(
            div()
                .text_size(theme::TEXT_SMALL)
                .font_weight(FontWeight::SEMIBOLD)
                .child(limit.label().to_string()),
        )
        .child(
            div()
                .text_size(theme::TEXT_TINY)
                .text_color(theme.secondary)
                .child(caption),
        )
}

/// The dial itself: a canvas for the two strokes with the percent laid over it
/// in an absolutely positioned overlay, because gpui canvases draw shapes and
/// not text.
fn dial(percent: f32, track: Rgba, arc: Rgba, limit: &Limit) -> impl IntoElement {
    let fraction = (percent / 100.0).clamp(0.0, 1.0);
    let draw_arc = percent > MIN_VISIBLE_PERCENT;

    div()
        .relative()
        .size(theme::RING_SIZE)
        .flex_shrink_0()
        .child(
            canvas(
                |_bounds, _window, _cx| (),
                move |bounds, _, window, _cx| {
                    if let Some(path) = circle(bounds) {
                        window.paint_path(path, Background::from(track));
                    }
                    if draw_arc {
                        if let Some(path) = sweep(bounds, fraction) {
                            window.paint_path(path, Background::from(arc));
                        }
                    }
                },
            )
            .size(theme::RING_SIZE),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .font_family(theme::MONO_FAMILY)
                .text_size(theme::TEXT_TITLE)
                .font_weight(FontWeight::SEMIBOLD)
                .child(format!("{}%", limit.percent_rounded())),
        )
}

/// The full track circle, drawn as two half arcs because a single `arc_to` back
/// to its own start point is degenerate.
fn circle(bounds: Bounds<Pixels>) -> Option<Path<Pixels>> {
    let (centre, radius) = geometry(bounds);
    let radii = point(radius, radius);
    let mut builder = PathBuilder::stroke(theme::RING_STROKE);
    builder.move_to(polar(centre, radius, 0.0));
    builder.arc_to(radii, px(0.), false, true, polar(centre, radius, 0.5));
    builder.arc_to(radii, px(0.), false, true, polar(centre, radius, 1.0));
    builder.build().ok()
}

/// The used fraction, from twelve o'clock clockwise.
fn sweep(bounds: Bounds<Pixels>, fraction: f32) -> Option<Path<Pixels>> {
    if fraction >= 1.0 {
        return circle(bounds);
    }
    let (centre, radius) = geometry(bounds);
    let radii = point(radius, radius);
    let mut builder = PathBuilder::stroke(theme::RING_STROKE);
    builder.move_to(polar(centre, radius, 0.0));
    // Past the halfway mark lyon needs the long way round the ellipse, or it
    // draws the complement of the arc we want.
    builder.arc_to(
        radii,
        px(0.),
        fraction > 0.5,
        true,
        polar(centre, radius, fraction),
    );
    builder.build().ok()
}

/// Centre and stroke-centre radius for a ring drawn into `bounds`: the stroke
/// straddles the path, so half of it has to stay inside the 56px box.
fn geometry(bounds: Bounds<Pixels>) -> (gpui::Point<Pixels>, Pixels) {
    let centre = bounds.center();
    let radius = bounds.size.width.min(bounds.size.height) / 2.0 - theme::RING_STROKE / 2.0;
    (centre, radius)
}

/// The point `fraction` of the way clockwise from twelve o'clock. Screen space
/// has y growing downward, so twelve o'clock is `-radius` on y.
fn polar(centre: gpui::Point<Pixels>, radius: Pixels, fraction: f32) -> gpui::Point<Pixels> {
    let angle = fraction * std::f32::consts::TAU;
    point(
        centre.x + radius * angle.sin(),
        centre.y - radius * angle.cos(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{size, Point};

    fn bounds() -> Bounds<Pixels> {
        Bounds {
            origin: Point {
                x: px(10.),
                y: px(20.),
            },
            size: size(theme::RING_SIZE, theme::RING_SIZE),
        }
    }

    #[test]
    fn the_row_is_as_tall_as_the_mockup() {
        // 10 + 56 + 6 + 19 + 6 + 18 + 14.
        assert_eq!(height(), 129.0);
    }

    #[test]
    fn the_radius_keeps_the_stroke_inside_the_box() {
        let (_, radius) = geometry(bounds());
        assert_eq!(radius, px(25.));
    }

    #[test]
    fn twelve_oclock_is_straight_up_from_the_centre() {
        let (centre, radius) = geometry(bounds());
        let top = polar(centre, radius, 0.0);
        assert!((f32::from(top.x) - f32::from(centre.x)).abs() < 1e-3);
        assert!((f32::from(top.y) - f32::from(centre.y - radius)).abs() < 1e-3);
    }

    #[test]
    fn a_quarter_turn_lands_at_three_oclock() {
        let (centre, radius) = geometry(bounds());
        let right = polar(centre, radius, 0.25);
        assert!((f32::from(right.x) - f32::from(centre.x + radius)).abs() < 1e-3);
        assert!((f32::from(right.y) - f32::from(centre.y)).abs() < 1e-3);
    }

    #[test]
    fn both_the_track_and_a_partial_arc_tessellate() {
        assert!(circle(bounds()).is_some());
        assert!(sweep(bounds(), 0.02).is_some());
        assert!(sweep(bounds(), 0.75).is_some());
        assert!(sweep(bounds(), 1.0).is_some());
    }
}
