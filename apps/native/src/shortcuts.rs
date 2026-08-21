use cutix_i18n::{t, t_args};
use gpui::{div, prelude::*, px, Context, Div, FontWeight, SharedString};

use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::keybindings::{Action, Category, Chord, Keybindings};
use crate::shell::Shell;
use crate::theme::{opacity, rem, Palette, RADIUS_LG, TEXT_LG, TEXT_SM, TEXT_XS};

const DIALOG_WIDTH_PX: f32 = 512.0;
const BODY_MAX_HEIGHT_PX: f32 = 460.0;
const BACKDROP_OPACITY: f32 = 0.55;

#[derive(Default)]
pub struct ShortcutsState {
    pub open: bool,
    pub recording: Option<Action>,
    pub notice: Option<SharedString>,
}

impl ShortcutsState {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.recording = None;
        self.notice = None;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.recording = None;
        self.notice = None;
    }
}

/// An action together with every chord currently bound to it.
pub type BoundAction = (Action, Vec<Chord>);

/// One section of the shortcut sheet: a category and the actions filed under it.
pub type ShortcutRow = (Category, Vec<BoundAction>);

pub fn rows(bindings: &Keybindings) -> Vec<ShortcutRow> {
    Category::ALL
        .into_iter()
        .filter_map(|category| {
            let actions: Vec<(Action, Vec<Chord>)> = Action::ALL
                .into_iter()
                .filter(|action| action.is_bindable() && action.category() == category)
                .map(|action| (action, bindings.chords_for(action)))
                .collect();
            (!actions.is_empty()).then_some((category, actions))
        })
        .collect()
}

pub fn conflict_message(chord: &Chord, existing: Action) -> String {
    t_args(
        "dialog.shortcuts.conflict",
        &[
            ("key", &chord.display()),
            ("action", &t(existing.description_key())),
        ],
    )
}

pub fn dialog(
    colors: Palette,
    bindings: &Keybindings,
    state: &ShortcutsState,
    cx: &mut Context<Shell>,
) -> Div {
    let groups = rows(bindings);
    let recording = state.recording;

    let body = groups
        .into_iter()
        .map(|(category, actions)| {
            let items = actions
                .into_iter()
                .map(|(action, chords)| row(colors, action, chords, recording == Some(action), cx))
                .collect::<Vec<_>>();

            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(rem(TEXT_XS))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.muted_foreground)
                        .child(t(category.key()).to_uppercase()),
                )
                .child(div().flex().flex_col().gap(px(4.0)).children(items))
        })
        .collect::<Vec<_>>();

    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(opacity(gpui::black(), BACKDROP_OPACITY))
        .child(
            div()
                .w(px(DIALOG_WIDTH_PX))
                .flex()
                .flex_col()
                .rounded(rem(RADIUS_LG))
                .border_1()
                .border_color(colors.border)
                .bg(colors.popover)
                .text_color(colors.popover_foreground)
                .shadow_lg()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .p(px(20.0))
                        .border_b_1()
                        .border_color(colors.border)
                        .child(
                            div()
                                .text_size(rem(TEXT_LG))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(t("dialog.shortcuts.title")),
                        )
                        .children(state.notice.clone().map(|notice| {
                            div()
                                .text_size(rem(TEXT_SM))
                                .text_color(colors.destructive)
                                .child(notice)
                        }))
                        .children(recording.map(|_| {
                            div()
                                .text_size(rem(TEXT_SM))
                                .text_color(colors.muted_foreground)
                                .child(t("dialog.shortcuts.recording"))
                        })),
                )
                .child(
                    div()
                        .id("shortcuts-body")
                        .flex()
                        .flex_col()
                        .gap(px(20.0))
                        .p(px(20.0))
                        .max_h(px(BODY_MAX_HEIGHT_PX))
                        .overflow_y_scroll()
                        .children(body),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(12.0))
                        .px(px(20.0))
                        .py(px(16.0))
                        .border_t_1()
                        .border_color(colors.border)
                        .child(
                            Button::new("shortcuts-reset", colors)
                                .variant(ButtonVariant::Destructive)
                                .label(t("dialog.shortcuts.reset"))
                                .build()
                                .on_click(cx.listener(|this: &mut Shell, _, _, cx| {
                                    this.reset_keybindings(cx);
                                })),
                        )
                        .child(
                            Button::new("shortcuts-close", colors)
                                .variant(ButtonVariant::Outline)
                                .label(t("common.close"))
                                .build()
                                .on_click(cx.listener(|this: &mut Shell, _, _, cx| {
                                    this.close_shortcuts(cx);
                                })),
                        ),
                ),
        )
}

fn row(
    colors: Palette,
    action: Action,
    chords: Vec<Chord>,
    recording: bool,
    cx: &mut Context<Shell>,
) -> Div {
    let mut keys: Vec<Div> = Vec::new();
    for (index, chord) in chords.iter().enumerate() {
        if index > 0 {
            keys.push(
                div()
                    .flex()
                    .items_center()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(t("common.or")),
            );
        }
        keys.push(
            div().child(
                Button::new(
                    SharedString::from(format!("shortcut-{}-{index}", action.id())),
                    colors,
                )
                .variant(if recording {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Outline
                })
                .size(ButtonSize::Sm)
                .label(chord.display())
                .build()
                .on_click(cx.listener(move |this: &mut Shell, _, _, cx| {
                    this.record_shortcut(action, cx);
                })),
            ),
        );
    }
    if chords.is_empty() {
        keys.push(
            div().child(
                Button::new(
                    SharedString::from(format!("shortcut-{}-unbound", action.id())),
                    colors,
                )
                .variant(if recording {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Ghost
                })
                .size(ButtonSize::Sm)
                .label(SharedString::from("\u{2014}"))
                .build()
                .on_click(cx.listener(move |this: &mut Shell, _, _, cx| {
                    this.record_shortcut(action, cx);
                })),
            ),
        );
    }

    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .text_size(rem(TEXT_SM))
                .child(t(action.description_key())),
        )
        .child(div().flex().items_center().gap(px(8.0)).children(keys))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_lists_every_bindable_action_exactly_once() {
        let bindings = Keybindings::defaults();
        let listed: Vec<Action> = rows(&bindings)
            .into_iter()
            .flat_map(|(_, actions)| actions.into_iter().map(|(action, _)| action))
            .collect();
        let expected: Vec<Action> = Action::ALL
            .into_iter()
            .filter(|action| action.is_bindable())
            .collect();
        assert_eq!(listed.len(), expected.len());
        for action in expected {
            assert!(listed.contains(&action), "{}", action.id());
        }
    }

    #[test]
    fn the_asset_actions_are_not_offered_for_rebinding() {
        let listed: Vec<Action> = rows(&Keybindings::defaults())
            .into_iter()
            .flat_map(|(_, actions)| actions.into_iter().map(|(action, _)| action))
            .collect();
        assert!(!listed.contains(&Action::RemoveMediaAsset));
        assert!(!listed.contains(&Action::RemoveMediaAssets));
    }

    #[test]
    fn actions_without_a_default_chord_still_get_a_row() {
        let bindings = Keybindings::defaults();
        let rows = rows(&bindings);
        let timeline = rows
            .iter()
            .find(|(category, _)| *category == Category::Timeline)
            .expect("timeline category");
        assert_eq!(timeline.1.len(), 1);
        assert_eq!(timeline.1[0].0, Action::ToggleBookmark);
        assert!(timeline.1[0].1.is_empty());
    }

    #[test]
    fn the_conflict_message_names_the_key_and_the_action_that_holds_it() {
        let message = conflict_message(&Chord::parse("s").unwrap(), Action::Split);
        assert!(message.contains('S'), "{message}");
        assert!(
            message.contains(&t(Action::Split.description_key())),
            "{message}"
        );
        assert!(!message.contains("{key}"), "{message}");
    }
}
