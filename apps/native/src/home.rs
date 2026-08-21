use cutix_i18n::t;
use gpui::{
    div, prelude::*, px, relative, svg, App, Context, Entity, FocusHandle, FontWeight, Render,
    SharedString, Window,
};

use crate::assets::icon;
use crate::interaction::mix;
use crate::interaction::Transitions;
use crate::state::{AppModel, Route};
use crate::theme::{opacity, rem, Palette, RADIUS_LG, RADIUS_MD, TEXT_LG, TEXT_SM, TEXT_XS};

const DESTINATIONS: [(&str, &str, Route); 2] = [
    ("home.projects.tile", "folder03", Route::Projects),
    ("home.library.tile", "oc-video", Route::Library),
];

pub struct HomeView {
    app: Entity<AppModel>,
    focus: FocusHandle,
    transitions: Transitions,
}

impl HomeView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            focus: cx.focus_handle(),
            transitions: Transitions::new(),
        }
    }

    fn colors(&self, cx: &App) -> Palette {
        self.app.read(cx).theme.root
    }

    fn card(
        &mut self,
        key: &'static str,
        glyph: &'static str,
        route: Route,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = self.colors(cx);
        let hover_key = format!("home-{key}");
        let lift = self.transitions.eased(&hover_key);
        let id = SharedString::from(hover_key.clone());

        div()
            .id(id)
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(24.0))
            .rounded(rem(RADIUS_LG))
            .border_1()
            .border_color(mix(colors.border, colors.primary, lift))
            .bg(mix(colors.card, colors.accent, 0.25 * lift))
            .cursor_pointer()
            .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                this.transitions.set(format!("home-{key}"), *hovered);
                cx.notify();
            }))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.transitions.set(format!("home-{key}"), false);
                this.app.update(cx, |model, cx| {
                    model.route = route;
                    cx.notify();
                });
            }))
            .child(
                div().flex().child(
                    div()
                        .w(px(44.0))
                        .h(px(44.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(rem(RADIUS_MD))
                        .bg(opacity(colors.primary, 0.10 + 0.10 * lift))
                        .child(
                            svg()
                                .size(px(22.0))
                                .flex_none()
                                .path(icon(glyph))
                                .text_color(colors.primary),
                        ),
                ),
            )
            .child(
                div()
                    .text_size(rem(TEXT_LG))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(t(key)),
            )
            .child(
                div()
                    .text_size(rem(TEXT_SM))
                    .text_color(colors.muted_foreground)
                    .child(t(&format!("{key}.hint"))),
            )
    }
    fn settings_button(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let lift = self.transitions.eased("home-settings");

        div()
            .id("home-settings")
            .flex()
            .items_center()
            .gap(px(8.0))
            .py(px(8.0))
            .px(px(14.0))
            .rounded(rem(RADIUS_LG))
            .border_1()
            .border_color(mix(gpui::transparent_black(), colors.border, lift))
            .bg(mix(gpui::transparent_black(), colors.card, lift))
            .cursor_pointer()
            .on_hover(cx.listener(|this: &mut Self, hovered: &bool, _, cx| {
                this.transitions.set("home-settings", *hovered);
                cx.notify();
            }))
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.transitions.set("home-settings", false);
                this.app.update(cx, |model, cx| {
                    model.settings_request = Some(String::new());
                    cx.notify();
                });
            }))
            .child(
                svg()
                    .size(px(15.0))
                    .path(icon("settings01"))
                    .text_color(mix(colors.muted_foreground, colors.foreground, lift)),
            )
            .child(
                div()
                    .text_size(rem(TEXT_SM))
                    .text_color(mix(colors.muted_foreground, colors.foreground, lift))
                    .child(t("common.settings")),
            )
    }
}

impl Render for HomeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.transitions.animating() {
            window.request_animation_frame();
        }
        let colors = self.colors(cx);
        let cards: Vec<_> = DESTINATIONS
            .iter()
            .map(|(key, glyph, route)| self.card(key, glyph, *route, cx))
            .collect();

        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(28.0))
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        svg()
                            .size(px(40.0))
                            .path(icon("cutix-logo"))
                            .text_color(colors.foreground),
                    )
                    .child(
                        div()
                            .text_size(rem(TEXT_LG))
                            .font_weight(FontWeight::MEDIUM)
                            .child(t("home.title")),
                    )
                    .child(
                        div()
                            .text_size(rem(TEXT_XS))
                            .text_color(colors.muted_foreground)
                            .child(t("home.subtitle")),
                    ),
            )
            .child(
                div()
                    .w(relative(0.72))
                    .max_w(px(760.0))
                    .flex()
                    .gap(px(16.0))
                    .children(cards),
            )
            .child(self.settings_button(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_destinations_are_translated_and_go_somewhere_that_is_not_home() {
        for (key, glyph, route) in DESTINATIONS {
            assert_ne!(t(key), key, "{key}");
            assert_ne!(t(&format!("{key}.hint")), format!("{key}.hint"), "{key}");
            assert!(crate::assets::icon_exists(glyph), "{glyph}");
            assert_ne!(
                route,
                Route::Home,
                "a card that goes nowhere is a dead card"
            );
        }
    }

    #[test]
    fn the_headings_are_translated() {
        for key in ["home.title", "home.subtitle"] {
            assert_ne!(t(key), key, "{key}");
        }
    }

    #[test]
    fn the_two_cards_do_not_lead_to_the_same_place() {
        assert_ne!(DESTINATIONS[0].2, DESTINATIONS[1].2);
    }
}
