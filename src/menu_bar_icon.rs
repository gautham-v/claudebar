//! The menu bar item's picture: a battery-style ring per limit, each with its
//! percentage and — when there is more than one — its tag, drawn at runtime
//! with Core Graphics rather than shipped as an asset, because the numbers
//! change on every fetch and a template image has to be regenerated to change.
//!
//! The whole item is one image, text included. A status item button has room
//! for one title and one image, so two rings cannot be a title plus an icon:
//! either everything is text or everything is drawn. Composing it here also
//! puts the spacing between a number and its ring under our own control
//! instead of a space character's width.
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
use objc2::runtime::{AnyObject, Bool};
use objc2_app_kit::{
    NSAttributedStringNSStringDrawing, NSColor, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSGraphicsContext, NSImage,
};
use objc2_core_graphics::{CGContext, CGLineCap};
use objc2_foundation::{NSAttributedString, NSDictionary, NSPoint, NSRect, NSSize};

/// Glyph box, in points. Square, and exactly the optical height of the system
/// glyphs beside it: the envelope and calendar outlines of its siblings are
/// 13pt tall, and a ring any taller reads as bigger than the battery.
pub const SIZE: f64 = 13.0;

/// Ring stroke, in points. A touch heavier than the calendar outline next
/// door: at the same weight the ring looked wispy beside the battery's solid
/// fill, and this is still thin enough that a 1% arc is a dot on the circle
/// rather than a blob.
pub const STROKE: f64 = 1.8;

/// How visible the unfilled part of the ring is. The track has to read as the
/// "rest of the circle" without competing with the arc.
const TRACK_ALPHA: f64 = 0.3;

/// The point size of the percentages. The menu bar's own font is 13pt, but the
/// system's battery percentage is set smaller, and this item sits right beside
/// it: measured off a 2x screen capture, the battery digits are 16px tall and
/// 12pt here came out at 18px, so 11pt is what lines them up.
pub const TEXT_POINT_SIZE: f64 = 11.0;

/// How present a tag is beside its number. The tag is scaffolding — it says
/// which window the number belongs to and then gets out of the way — so it is
/// the same size, drawn lighter. Alpha rather than a lighter colour because
/// the alpha survives template tinting, which a colour does not.
const TAG_ALPHA: f64 = 0.6;

/// Between a limit's tag and its number.
const TAG_GAP: f64 = 3.0;
/// Between a number and the ring it belongs to.
const RING_GAP: f64 = 3.0;
/// Between one limit and the next. Wider than the gaps inside a limit, so the
/// item reads as two or three groups rather than a row of loose numbers.
const PART_GAP: f64 = 7.0;

/// The ink the menu bar is drawing its own items in.
///
/// It only matters for an item that cannot be a template — see
/// [`item_image`] — and it is not simply light mode versus dark: the menu bar
/// over a pale wallpaper draws in dark ink even in dark mode, which is what
/// the status item button's own appearance reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuBarInk {
    /// A dark menu bar, drawing in white.
    White,
    /// A light menu bar, drawing in black.
    Black,
}

/// One limit, as the menu bar item draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemPart {
    /// The window's short name, drawn before the number. `None` when the item
    /// is showing a single limit, or the user turned labels off.
    pub tag: Option<String>,
    pub percent: f32,
    /// [`crate::model::Limit::is_low`]: this window is nearly spent, so its
    /// number and ring are drawn in the system red.
    pub low: bool,
}

/// What a laid-out piece of the item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    Tag,
    Number,
    Ring,
}

/// A piece of the item and how wide it is, before it has been given an x.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// Which limit this piece belongs to; the gap between two limits is wider
    /// than the gaps inside one.
    pub part: usize,
    pub kind: SegmentKind,
    pub width: f64,
}

/// Lay the segments out left to right. Returns the item's width and each
/// segment's left edge. Pure, so the spacing rules are tested without AppKit.
pub fn place(segments: &[Segment]) -> (f64, Vec<f64>) {
    let mut x = 0.0;
    let mut lefts = Vec::with_capacity(segments.len());
    let mut previous: Option<Segment> = None;
    for segment in segments {
        if let Some(previous) = previous {
            x += if previous.part != segment.part {
                PART_GAP
            } else if segment.kind == SegmentKind::Ring {
                RING_GAP
            } else {
                TAG_GAP
            };
        }
        lefts.push(x);
        x += segment.width;
        previous = Some(*segment);
    }
    (x, lefts)
}

/// One drawn thing, with the x it goes at.
enum Ink {
    Text {
        x: f64,
        text: Retained<NSAttributedString>,
    },
    Ring {
        x: f64,
        percent: f32,
        low: bool,
    },
}

/// The whole menu bar item as one [`NSImage`]: every part's tag, number and
/// ring, in the order given.
///
/// The image is a template unless something in it is red — a template throws
/// the colours away and keeps the alpha, which is how the item gets tinted for
/// the dark, light and reduced-transparency menu bars for free.
///
/// One low window spoils that: red has to survive, so the image stops being a
/// template and every colour in it becomes our problem. The red parts are
/// `NSColor::systemRedColor`, which is right under any appearance, and the
/// rest are drawn in `ink` — the menu bar's own ink, which the caller reads
/// off the status item button. `NSColor::labelColor` is not an option here:
/// inside a drawing handler it resolves against the *app's* appearance, which
/// is light, and painted near-black numbers onto a dark menu bar.
pub fn item_image(
    parts: &[ItemPart],
    show_percent: bool,
    show_rings: bool,
    ink: MenuBarInk,
) -> Retained<NSImage> {
    // Nothing drawn is no click target, so the ring stands in whenever the
    // number is the only thing that was asked for and then turned off.
    let show_rings = show_rings || !show_percent;
    let template = !parts.iter().any(|part| part.low);

    let mut segments = Vec::new();
    let mut texts = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        if let Some(tag) = &part.tag {
            let text = text_run(tag, part.low, template, ink, TAG_ALPHA);
            segments.push(Segment {
                part: index,
                kind: SegmentKind::Tag,
                width: text.size().width,
            });
            texts.push(Some(text));
        }
        if show_percent {
            let label = format!("{}%", clamp_percent(part.percent).round() as u32);
            let text = text_run(&label, part.low, template, ink, 1.0);
            segments.push(Segment {
                part: index,
                kind: SegmentKind::Number,
                width: text.size().width,
            });
            texts.push(Some(text));
        }
        if show_rings {
            segments.push(Segment {
                part: index,
                kind: SegmentKind::Ring,
                width: SIZE,
            });
            texts.push(None);
        }
    }

    let (width, lefts) = place(&segments);
    let height = segments
        .iter()
        .zip(&texts)
        .filter_map(|(_, text)| text.as_ref().map(|t| t.size().height))
        .fold(SIZE, f64::max);

    let inks: Vec<Ink> = segments
        .iter()
        .zip(texts)
        .zip(&lefts)
        .map(|((segment, text), x)| match text {
            Some(text) => Ink::Text { x: *x, text },
            None => Ink::Ring {
                x: *x,
                percent: parts[segment.part].percent,
                low: parts[segment.part].low,
            },
        })
        .collect();

    let handler = RcBlock::new(move |_dirty: NSRect| -> Bool {
        draw_item(&inks, height, template, ink);
        Bool::YES
    });
    let image = NSImage::imageWithSize_flipped_drawingHandler(
        NSSize::new(width.max(SIZE), height),
        false,
        &handler,
    );
    image.setTemplate(template);
    image
}

/// One run of text in the menu bar font, coloured for the image it is going
/// into: black for a template (AppKit tints the alpha), otherwise the system
/// red or the menu bar's label colour.
fn text_run(
    label: &str,
    low: bool,
    template: bool,
    ink: MenuBarInk,
    alpha: f64,
) -> Retained<NSAttributedString> {
    let font = NSFont::menuBarFontOfSize(TEXT_POINT_SIZE);
    let colour = ink_colour(low, template, ink, alpha);
    // Safety: `NSFontAttributeName` documents its value as an `NSFont` and
    // `NSForegroundColorAttributeName` as an `NSColor`, which is what we pass.
    unsafe {
        let attrs = NSDictionary::from_slices(
            &[NSFontAttributeName, NSForegroundColorAttributeName],
            &[&*font as &AnyObject, &*colour as &AnyObject],
        );
        NSAttributedString::new_with_attributes(
            &objc2_foundation::NSString::from_str(label),
            &attrs,
        )
    }
}

/// The ink for one piece of the item.
fn ink_colour(low: bool, template: bool, ink: MenuBarInk, alpha: f64) -> Retained<NSColor> {
    let base = if low {
        // From `NSColor` rather than a literal, so it tracks the user's
        // accessibility settings.
        NSColor::systemRedColor()
    } else if template {
        // A template keeps only the alpha, so the colour just has to be
        // opaque ink.
        NSColor::blackColor()
    } else {
        match ink {
            MenuBarInk::White => NSColor::whiteColor(),
            MenuBarInk::Black => NSColor::blackColor(),
        }
    };
    if alpha < 1.0 {
        base.colorWithAlphaComponent(alpha)
    } else {
        base
    }
}

/// Draw every piece into whatever context AppKit made current for the handler.
fn draw_item(inks: &[Ink], height: f64, template: bool, ink: MenuBarInk) {
    for piece in inks {
        match piece {
            Ink::Text { x, text } => {
                // Vertically centred: `drawAtPoint` takes the bottom-left of
                // the run's own box in this bottom-left-origin context.
                let y = (height - text.size().height) / 2.0;
                text.drawAtPoint(NSPoint::new(*x, y));
            }
            Ink::Ring { x, percent, low } => {
                draw_ring(*x, height / 2.0, *percent, *low, template, ink)
            }
        }
    }
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

/// Centre of a ring whose glyph box starts at `x`, in the image's own
/// bottom-left-origin coordinates, vertically centred on `mid_y`.
pub fn center(x: f64, mid_y: f64) -> (f64, f64) {
    (x + SIZE / 2.0, mid_y)
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

/// Draw one ring into whatever context AppKit has made current for the
/// handler, with its glyph box starting at `x`.
fn draw_ring(x: f64, mid_y: f64, percent: f32, low: bool, template: bool, ink: MenuBarInk) {
    let Some(ctx) = NSGraphicsContext::currentContext() else {
        return;
    };
    let cg = ctx.CGContext();
    let cg = Some(&*cg);

    let (cx, cy) = center(x, mid_y);
    let r = radius();

    CGContext::set_should_antialias(cg, true);
    CGContext::set_line_width(cg, STROKE);
    CGContext::set_line_cap(cg, CGLineCap::Round);

    // The track: the full circle, faint. Butt caps would be visible where a
    // closed circle's ends meet, so the cap style is left as set above and the
    // path is closed by `add_arc` itself.
    set_stroke_colour(low, template, ink, TRACK_ALPHA);
    CGContext::begin_path(cg);
    CGContext::add_arc(cg, cx, cy, r, 0.0, std::f64::consts::TAU, 0);
    CGContext::stroke_path(cg);

    if !has_arc(percent) {
        return;
    }

    set_stroke_colour(low, template, ink, 1.0);
    CGContext::begin_path(cg);
    CGContext::add_arc(cg, cx, cy, r, arc_start_angle(), arc_end_angle(percent), 1);
    CGContext::stroke_path(cg);
}

/// The ink for a ring. `setStroke` writes into the current graphics context,
/// which is the one the CG path stroking above reads, so this and the text
/// runs take their colours from the same place.
fn set_stroke_colour(low: bool, template: bool, ink: MenuBarInk, alpha: f64) {
    ink_colour(low, template, ink, alpha).setStroke();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_sits_inside_the_glyph_box() {
        let (cx, cy) = center(0.0, SIZE / 2.0);
        assert!(cx - radius() - STROKE / 2.0 >= 0.0);
        assert!(cy + radius() + STROKE / 2.0 <= SIZE);
    }

    /// A second limit's glyph box starts where the layout put it, not at zero.
    #[test]
    fn a_rings_centre_follows_its_box() {
        let (cx, cy) = center(40.0, 7.0);
        assert_eq!(cx, 40.0 + SIZE / 2.0);
        assert_eq!(cy, 7.0);
    }

    fn segment(part: usize, kind: SegmentKind, width: f64) -> Segment {
        Segment { part, kind, width }
    }

    /// One limit: number, a small gap, ring — the shape the item has always
    /// had.
    #[test]
    fn one_limit_is_a_number_then_its_ring() {
        let (width, lefts) = place(&[
            segment(0, SegmentKind::Number, 20.0),
            segment(0, SegmentKind::Ring, SIZE),
        ]);
        assert_eq!(lefts, vec![0.0, 20.0 + RING_GAP]);
        assert_eq!(width, 20.0 + RING_GAP + SIZE);
    }

    /// Two limits: the gap between them is wider than the gaps inside either,
    /// so a tag reads as belonging to the number after it.
    #[test]
    fn limits_are_spaced_further_apart_than_their_own_pieces() {
        let (_, lefts) = place(&[
            segment(0, SegmentKind::Tag, 14.0),
            segment(0, SegmentKind::Number, 20.0),
            segment(0, SegmentKind::Ring, SIZE),
            segment(1, SegmentKind::Tag, 16.0),
            segment(1, SegmentKind::Number, 24.0),
            segment(1, SegmentKind::Ring, SIZE),
        ]);
        let inside = lefts[1] - (lefts[0] + 14.0);
        let between = lefts[3] - (lefts[2] + SIZE);
        assert_eq!(inside, TAG_GAP);
        assert_eq!(between, PART_GAP);
        assert!(between > inside);
    }

    #[test]
    fn an_empty_item_has_no_width() {
        assert_eq!(place(&[]), (0.0, vec![]));
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
