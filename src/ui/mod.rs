//! gpui views for the popover.
//!
//! `popover` is the root view; `stats` is the leaf it composes for the two
//! blocks read out of the local session logs. All colors and sizes come from
//! [`theme`] — no literals in the views.

pub mod popover;
pub mod provider;
pub mod stats;
pub mod theme;
