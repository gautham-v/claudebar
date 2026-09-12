//! The two local blocks — sections 4 and 5 of `docs/mockup-popover.dc.html`.
//!
//! [`today`] is the "Today" block: the date on the right of the title, three
//! stat tiles (sessions, tool calls, tokens), and one bar per model that did
//! real work today. [`week`] is the "Last 7 days" block: seven bars scaled to
//! the busiest day with today picked out in the accent, over the weekday row.
//!
//! Both read everything from the [`Popover`]'s provider, and both have a
//! `*_height` twin so [`Popover::preferred_height`] can size the window to what
//! is actually drawn.
//!
//! [`Popover::preferred_height`]: crate::ui::popover::Popover::preferred_height

use gpui::{div, px, Context, FontWeight, IntoElement, ParentElement, Styled};

use crate::model::{compact_count, model_display_name, DayStats};
use crate::ui::popover::Popover;
use crate::ui::theme::{self, Theme};

// ── Metrics, kept next to the layout they describe ───────────────────────────

/// Padding above and below either block (the mockup's `10px 14px 12px`).
const BLOCK_PAD_TOP: f32 = 10.0;
const BLOCK_PAD_BOTTOM: f32 = 12.0;
/// The gap between a block's title row and the rest of it.
const BLOCK_GAP: f32 = 10.0;
/// The 13px semibold title line. gpui lays text out at roughly 1.6x the font
/// size, which is what every line constant here is.
const TITLE_LINE: f32 = 21.0;
/// A stat tile: a 15px number over an 11px label, one pixel apart.
const STAT_NUMBER_LINE: f32 = 24.0;
const STAT_LABEL_LINE: f32 = 18.0;
const STAT_TILE: f32 = STAT_NUMBER_LINE + 1.0 + STAT_LABEL_LINE;
/// A model row: the name line over its bar.
const MODEL_LABEL_LINE: f32 = 19.0;
const MODEL_ROW_GAP: f32 = 5.0;
const MODEL_ROW: f32 = MODEL_LABEL_LINE + MODEL_ROW_GAP + theme::MODEL_BAR_HEIGHT_PX;
/// The gap between the parts of the week block, which is tighter than the
/// today block's.
const WEEK_GAP: f32 = 6.0;
/// The 10px weekday initials under the spark bars.
const WEEKDAY_LINE: f32 = 16.0;

/// A model has to account for this much of the day's tokens to earn a row;
/// below it the bar would be invisible and the list would be noise.
const MODEL_MIN_PERCENT: u32 = 1;

/// The "Today" block.
pub fn today(popover: &Popover, _cx: &mut Context<Popover>) -> impl IntoElement {
    let theme = popover.theme();
    let day = popover.today_stats();
    let date = popover.now().format("%a, %b %-d").to_string();
    let (sessions, tool_calls, tokens) = match &day {
        Some(d) => (
            d.sessions.to_string(),
            d.tool_calls.to_string(),
            compact_count(d.tokens),
        ),
        // No scan has landed yet: the tiles keep their shape and show nothing,
        // so the popover does not jump when the numbers arrive.
        None => ("—".into(), "—".into(), "—".into()),
    };
    let models = day.as_ref().map(model_shares).unwrap_or_default();

    div()
        .flex()
        .flex_col()
        .gap(px(BLOCK_GAP))
        .px(theme::PAD_X)
        .pt(px(BLOCK_PAD_TOP))
        .pb(px(BLOCK_PAD_BOTTOM))
        .child(title_row(theme, "Today", date))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(8.))
                .child(stat_tile(theme, sessions, "sessions"))
                .child(stat_tile(theme, tool_calls, "tool calls"))
                .child(stat_tile(theme, tokens, "tokens")),
        )
        .children((!models.is_empty()).then(|| {
            div().flex().flex_col().gap(px(MODEL_ROW_GAP)).children(
                models
                    .iter()
                    .map(|(name, percent)| model_row(theme, name, *percent))
                    .collect::<Vec<_>>(),
            )
        }))
}

/// Height of the "Today" block, which grows by one row per model shown.
pub fn today_height(popover: &Popover) -> f32 {
    let models = popover
        .today_stats()
        .as_ref()
        .map(model_shares)
        .unwrap_or_default()
        .len();
    let model_block = if models == 0 {
        0.0
    } else {
        BLOCK_GAP + models as f32 * MODEL_ROW + (models - 1) as f32 * MODEL_ROW_GAP
    };
    BLOCK_PAD_TOP + TITLE_LINE + BLOCK_GAP + STAT_TILE + model_block + BLOCK_PAD_BOTTOM
}

/// The "Last 7 days" block.
pub fn week(popover: &Popover, _cx: &mut Context<Popover>) -> impl IntoElement {
    let theme = popover.theme();
    let days = popover
        .local_stats()
        .map(|s| s.days.clone())
        .unwrap_or_default();
    let busiest = days.iter().map(|d| d.tokens).max().unwrap_or(0);
    let last = days.len().saturating_sub(1);

    div()
        .flex()
        .flex_col()
        .gap(px(WEEK_GAP))
        .px(theme::PAD_X)
        .pt(px(BLOCK_PAD_TOP))
        .pb(px(BLOCK_PAD_BOTTOM))
        .child(title_row(theme, "Last 7 days", "tokens per day".into()))
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
        .child(
            div()
                .flex()
                .flex_row()
                .gap(theme::SPARK_GAP)
                .text_size(theme::TEXT_MICRO)
                .text_color(theme.tertiary)
                .children(
                    days.iter()
                        .map(|day| {
                            div()
                                .flex_grow()
                                .flex_basis(px(0.))
                                .text_center()
                                .child(day.date.format("%a").to_string())
                        })
                        .collect::<Vec<_>>(),
                ),
        )
}

/// Height of the "Last 7 days" block. Constant: it always draws seven bars.
pub fn week_height() -> f32 {
    BLOCK_PAD_TOP
        + TITLE_LINE
        + WEEK_GAP
        + f32::from(theme::SPARK_MAX_HEIGHT)
        + WEEK_GAP
        + WEEKDAY_LINE
        + BLOCK_PAD_BOTTOM
}

/// A block's title with its caption pushed to the right.
fn title_row(theme: Theme, title: &'static str, caption: String) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_baseline()
        .justify_between()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(theme::TEXT_TITLE)
                .child(title),
        )
        .child(
            div()
                .text_size(theme::TEXT_TINY)
                .text_color(theme.secondary)
                .child(caption),
        )
}

/// One of the three tiles: a mono number over a muted label. `flex_basis(0)`
/// makes the three share the width evenly however wide their numbers are.
fn stat_tile(theme: Theme, value: String, label: &'static str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_grow()
        .flex_basis(px(0.))
        .gap(px(1.))
        .child(
            div()
                .font_family(theme::MONO_FAMILY)
                .text_size(theme::TEXT_STAT)
                .font_weight(FontWeight::SEMIBOLD)
                .child(value),
        )
        .child(
            div()
                .text_size(theme::TEXT_TINY)
                .text_color(theme.secondary)
                .child(label),
        )
}

/// A model's name, its share of the day, and the bar under both.
fn model_row(theme: Theme, name: &str, percent: u32) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(MODEL_ROW_GAP))
        .child(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .text_size(theme::TEXT_SMALL)
                .child(div().child(name.to_string()))
                .child(
                    div()
                        .font_family(theme::MONO_FAMILY)
                        .text_color(theme.secondary)
                        .child(format!("{percent}%")),
                ),
        )
        .child(
            div()
                .w_full()
                .h(theme::MODEL_BAR_HEIGHT)
                .rounded(theme::MODEL_BAR_RADIUS)
                .bg(theme.separator)
                .overflow_hidden()
                .child(
                    div()
                        .w(gpui::relative(percent as f32 / 100.0))
                        .h_full()
                        .rounded(theme::MODEL_BAR_RADIUS)
                        .bg(theme.model_bar),
                ),
        )
}

/// One day's bar, scaled against the busiest day of the seven.
fn spark_bar(theme: Theme, tokens: u64, busiest: u64, is_today: bool) -> impl IntoElement {
    let height = if busiest == 0 {
        0.0
    } else {
        f32::from(theme::SPARK_MAX_HEIGHT) * (tokens as f32 / busiest as f32)
    };
    let fill = if is_today {
        theme.accent
    } else {
        theme.spark_past()
    };
    div()
        .flex()
        .flex_col()
        .justify_end()
        .flex_grow()
        .flex_basis(px(0.))
        .h(theme::SPARK_MAX_HEIGHT)
        .child(div().h(px(height)).rounded(theme::SPARK_RADIUS).bg(fill))
}

/// The models worth a row today: display name and whole-number share, largest
/// first, anything under [`MODEL_MIN_PERCENT`] dropped.
fn model_shares(day: &DayStats) -> Vec<(String, u32)> {
    if day.tokens == 0 {
        return Vec::new();
    }
    let mut shares: Vec<(String, u32)> = day
        .tokens_by_model
        .iter()
        .map(|(id, tokens)| {
            let percent = (*tokens as f64 * 100.0 / day.tokens as f64).round() as u32;
            (model_display_name(id), percent)
        })
        .filter(|(_, percent)| *percent >= MODEL_MIN_PERCENT)
        .collect();
    shares.sort_by_key(|(_, percent)| std::cmp::Reverse(*percent));
    shares
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn day(tokens: u64, by_model: &[(&str, u64)]) -> DayStats {
        DayStats {
            date: NaiveDate::from_ymd_opt(2026, 9, 12).unwrap(),
            sessions: 14,
            tool_calls: 412,
            tokens,
            tokens_by_model: by_model
                .iter()
                .map(|(id, n)| ((*id).to_string(), *n))
                .collect(),
        }
    }

    #[test]
    fn model_shares_are_percentages_largest_first() {
        let shares = model_shares(&day(
            1_000,
            &[("claude-opus-5", 130), ("claude-fable-5-1", 860)],
        ));
        assert_eq!(
            shares,
            vec![("Fable".to_string(), 86), ("Opus".to_string(), 13)]
        );
    }

    #[test]
    fn a_sliver_of_a_model_gets_no_row() {
        let shares = model_shares(&day(
            10_000,
            &[("claude-fable-5-1", 9_980), ("claude-haiku-4-5", 20)],
        ));
        assert_eq!(shares.len(), 1);
    }

    #[test]
    fn an_idle_day_has_no_model_rows() {
        assert!(model_shares(&day(0, &[("claude-opus-5", 0)])).is_empty());
    }

    #[test]
    fn the_week_block_is_as_tall_as_the_mockup() {
        // 10 + 21 + 6 + 28 + 6 + 16 + 12.
        assert_eq!(week_height(), 99.0);
    }
}
