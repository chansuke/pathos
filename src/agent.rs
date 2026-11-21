//! The proposals of agents in the window (ADR 35).

use gpui::{Context, Div, FontWeight, MouseButton, MouseDownEvent, div, prelude::*, px, rgb, rgba};
use pathos_agent::Proposal;

use crate::layout::TIMELINE_TOP;
use crate::{
    ACCENT, BACKGROUND, CONTROL, CONTROL_BORDER, GRID, MONO, PANEL, Pathos, SOFT_TEXT, SUBTLE_TEXT,
    key_cap,
};

/// The widest a proposal is shown.
const CARD_WIDTH: f32 = 400.0;
/// The most changes a proposal shows; the rest are counted.
const SHOWN_CHANGES: usize = 8;

/// The oldest pending proposal at the top right of the lanes, with the
/// changes it makes, the number of proposals and the keys that apply or
/// dismiss it.
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
        .child("AGENT PROPOSAL")
        .children((pending > 1).then(|| format!("1 OF {pending}")));
    let summary = (!proposal.summary.is_empty()).then(|| {
        div()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .child(proposal.summary.clone())
    });
    let changes = div()
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
        .children((more > 0).then(|| div().child(format!("and {more} more"))));
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
        .child(button("Apply", "Enter", true).on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.apply_proposal(cx);
            }),
        ));
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
        .children(summary)
        .child(changes)
        .child(buttons)
        // A click on the card is not a click on the lanes under it.
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
        )
}
