//! The proposals of agents and the findings of reviews in the window (ADR 35,
//! ADR 36).

use gpui::{Context, Div, FontWeight, MouseButton, MouseDownEvent, div, prelude::*, px, rgb, rgba};
use pathos_agent::{Anchor, Proposal};
use pathos_core::PPQ;

use crate::layout::TIMELINE_TOP;
use crate::{
    ACCENT, BACKGROUND, CONTROL, CONTROL_BORDER, GRID, MONO, PANEL, Pathos, SOFT_TEXT, SUBTLE_TEXT,
    key_cap,
};

/// The widest a proposal is shown.
const CARD_WIDTH: f32 = 400.0;
/// The most changes a proposal shows; the rest are counted.
const SHOWN_CHANGES: usize = 8;

/// The ticks of a bar, as the ruler counts them.
const BAR: u64 = 4 * PPQ;

/// Where a finding is, as the window names tracks and bars.
pub fn anchor_label(anchor: &Anchor) -> String {
    let place = match anchor.track {
        Some(track) => format!("Track {}", track + 1),
        None => "Master".into(),
    };
    let (first, last) = (anchor.start / BAR + 1, (anchor.end - 1) / BAR + 1);
    if first == last {
        format!("{place}, bar {first}")
    } else {
        format!("{place}, bars {first} to {last}")
    }
}

/// The oldest pending proposal at the top right of the lanes, with where it
/// points if it is a finding, the changes it makes, the number of proposals
/// and the keys that apply or dismiss it. A finding without commands can only
/// be dismissed.
pub fn proposal_card(
    proposal: &Proposal,
    pending: usize,
    width: f32,
    cx: &mut Context<Pathos>,
) -> Div {
    let card_width = CARD_WIDTH.min(width - 32.0).max(0.0);
    let more = proposal.changes.len().saturating_sub(SHOWN_CHANGES);
    let header = div()
        .flex()
        .justify_between()
        .text_size(px(10.0))
        .text_color(rgb(SUBTLE_TEXT))
        .child(if proposal.anchor.is_some() {
            "REVIEW FINDING"
        } else {
            "AGENT PROPOSAL"
        })
        .children((pending > 1).then(|| format!("1 OF {pending}")));
    let summary = (!proposal.summary.is_empty()).then(|| {
        div()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .child(proposal.summary.clone())
    });
    let anchor = proposal.anchor.map(|anchor| {
        div()
            .text_size(px(11.0))
            .text_color(rgb(SOFT_TEXT))
            .child(anchor_label(&anchor))
    });
    let changes = (!proposal.changes.is_empty()).then(|| {
        div()
            .flex()
            .flex_col()
            .gap_0p5()
            .p_2()
            .rounded(px(4.0))
            .bg(rgb(BACKGROUND))
            .font_family(MONO)
            .text_size(px(11.0))
            .text_color(rgb(SOFT_TEXT))
            .children(
                proposal
                    .changes
                    .iter()
                    .take(SHOWN_CHANGES)
                    .map(|change| div().child(change.clone())),
            )
            .children((more > 0).then(|| div().child(format!("and {more} more"))))
    });
    let button = |label: &'static str, key: &'static str, primary: bool| {
        let button = div()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded(px(6.0))
            .border_1()
            .text_sm()
            .child(label)
            .child(key_cap(key));
        if primary {
            button
                .bg(rgba(ACCENT << 8 | 0x29))
                .border_color(rgba(ACCENT << 8 | 0xb3))
        } else {
            button
                .bg(rgb(CONTROL))
                .border_color(rgb(CONTROL_BORDER))
                .text_color(rgb(SOFT_TEXT))
        }
    };
    let buttons = div()
        .flex()
        .justify_end()
        .gap_2()
        .child(button("Dismiss", "Esc", false).on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.dismiss_proposal(cx);
            }),
        ))
        .children((!proposal.commands.is_empty()).then(|| {
            button("Apply", "Enter", true).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.apply_proposal(cx);
                }),
            )
        }));
    div()
        .absolute()
        .top(px(TIMELINE_TOP + 12.0))
        .left(px(width - card_width - 16.0))
        .w(px(card_width))
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(GRID))
        .bg(rgb(PANEL))
        .shadow_lg()
        .child(header)
        .children(anchor)
        .children(summary)
        .children(changes)
        .child(buttons)
        // A click on the card is not a click on the lanes under it.
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_anchor_is_named_by_its_track_and_bars() {
        let anchor = |track, start, end| Anchor { track, start, end };
        assert_eq!(anchor_label(&anchor(Some(0), 0, BAR)), "Track 1, bar 1");
        assert_eq!(
            anchor_label(&anchor(None, BAR + PPQ, 3 * BAR + 1)),
            "Master, bars 2 to 4"
        );
    }
}
