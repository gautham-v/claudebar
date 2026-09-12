//! The two local blocks — sections 4 and 5 of `docs/mockup-popover-v2.dc.html`.
//!
//! [`today`] is the "Today" block: a bold header over three key-value rows
//! (sessions, tool calls, tokens), the values right-aligned in tabular
//! numerals. [`week`] is a single row: "Last 7 days" on the left and a seven
//! bar sparkline on the right, scaled to the busiest day with today picked out
//! of the six faded ones.
//!
//! Both read everything from the [`Popover`]'s provider, and both have a
//! `*_height` twin so [`Popover::preferred_height`] can size the window to what
//! is actually drawn. Neither block needs a token, so both render signed out.
//!
//! [`Popover::preferred_height`]: crate::ui::popover::Popover::preferred_height

use gpui::{div, px, Context, IntoElement, ParentElement, SharedString, Styled};

use crate::model::compact_count;
use crate::ui::popover::{self, Popover};
use crate::ui::theme::{self, Theme};

/// The "Today" block: its header and the three rows under it.
pub fn today(popover: &Popover, _cx: &mut Context<Popover>) -> impl IntoElement {
    let theme = popover.theme();
    let day = popover.today_stats();
    let (sessions, tool_calls, tokens) = match &day {
        Some(d) => (
            d.sessions.to_string(),
            d.tool_calls.to_string(),
            compact_count(d.tokens),
        ),
        // No scan has landed yet: the rows keep their shape and show a dash,
        // so the popover does not jump when the numbers arrive.
        None => ("—".into(), "—".into(), "—".into()),
    };

    div()
        .flex()
        .flex_col()
        .child(popover::section_header("Today"))
        .child(value_row(theme, "Sessions", sessions))
        .child(value_row(theme, "Tool calls", tool_calls))
        .child(value_row(theme, "Tokens", tokens))
}

/// Height of the "Today" block. Constant: it is always a header and the same
/// three rows, whatever the day held.
pub fn today_height() -> f32 {
    popover::SECTION_HEADER_HEIGHT + 3.0 * popover::ROW_HEIGHT
}

/// The "Last 7 days" row: the label and the sparkline beside it.
pub fn week(popover: &Popover, _cx: &mut Context<Popover>) -> impl IntoElement {
    let theme = popover.theme();
    let days = popover
        .local_stats()
        .map(|s| s.days.clone())
        .unwrap_or_default();
    let busiest = days.iter().map(|d| d.tokens).max().unwrap_or(0);
    let last = days.len().saturating_sub(1);

    popover::row_frame()
        .justify_between()
        .items_center()
        .text_size(theme::TEXT_BODY)
        .child(div().line_height(theme::LINE_TITLE).child("Last 7 days"))
        .child(
            div()
                .flex()
                .flex_row()
                .items_end()
                .gap(theme::SPARK_GAP)
                .h(theme::SPARK_MAX_HEIGHT)
                .children(
                    days.iter()
                        .enumerate()
                        .map(|(i, day)| spark_bar(theme, day.tokens, busiest, i == last))
                        .collect::<Vec<_>>(),
                ),
        )
}

/// Height of the "Last 7 days" row. The sparkline is shorter than the line of
/// text beside it, so the row is as tall as any other.
pub fn week_height() -> f32 {
    popover::ROW_HEIGHT
}

/// One "Sessions 14" row: the label in ink, the value secondary and right
/// aligned. The numbers are mono so the three values line up as a column.
fn value_row(theme: Theme, label: &'static str, value: String) -> impl IntoElement {
    popover::row_frame()
        .justify_between()
        .items_baseline()
        .text_size(theme::TEXT_BODY)
        .child(div().line_height(theme::LINE_TITLE).child(label))
        .child(
            div()
                .font_family(theme::MONO_FAMILY)
                .line_height(theme::LINE_TITLE)
                .text_color(theme.secondary)
                .child(SharedString::from(value)),
        )
}

/// One day's bar, scaled against the busiest day of the seven. Today is drawn
/// in ink and the six before it far back, so the row reads as "and today".
fn spark_bar(theme: Theme, tokens: u64, busiest: u64, is_today: bool) -> impl IntoElement {
    let height = if busiest == 0 {
        0.0
    } else {
        theme::SPARK_MAX_HEIGHT_PX * (tokens as f32 / busiest as f32)
    };
    let fill = if is_today {
        theme.text
    } else {
        theme.spark_past()
    };
    div()
        .flex()
        .flex_col()
        .justify_end()
        .w(theme::SPARK_BAR_WIDTH)
        .h(theme::SPARK_MAX_HEIGHT)
        .child(div().h(px(height)).rounded(theme::SPARK_RADIUS).bg(fill))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Today block is a header over three rows, and the week block is one
    /// row: the arithmetic `preferred_height` adds up.
    #[test]
    fn the_blocks_are_as_tall_as_the_mockup() {
        // 23 for the header, 23 per row.
        assert_eq!(today_height(), 23.0 + 3.0 * 23.0);
        assert_eq!(week_height(), 23.0);
    }
}
