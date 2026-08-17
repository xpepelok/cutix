#![allow(dead_code)]

use gpui::{div, prelude::*, px, Div, FontWeight, Hsla, SharedString, Stateful, StyleRefinement};

use crate::assets::icon;
use crate::interaction::{mix, OverlayFrame, OverlaySide};
use crate::theme::{opacity, rem, Palette, RADIUS_MD, RADIUS_SM, TEXT_SM};

pub const BUTTON_GAP_PX: f32 = 8.0;
pub const BUTTON_ICON_PX: f32 = 16.0;
pub const TEXT_HOVER_OPACITY: f32 = 0.75;
pub const PRESSED_OPACITY: f32 = 0.7;
pub const DISABLED_OPACITY: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    Default,
    Background,
    Destructive,
    DestructiveForeground,
    Caution,
    Outline,
    Secondary,
    Text,
    Ghost,
    Link,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonSize {
    Default,
    Sm,
    Lg,
    Icon,
    Text,
}

impl ButtonSize {
    pub fn height(self) -> Option<f32> {
        match self {
            ButtonSize::Default => Some(36.0),
            ButtonSize::Sm | ButtonSize::Icon => Some(28.0),
            ButtonSize::Lg => Some(40.0),
            ButtonSize::Text => None,
        }
    }

    pub fn padding(self) -> (f32, f32) {
        match self {
            ButtonSize::Default => (8.0, 16.0),
            ButtonSize::Sm => (4.0, 10.0),
            ButtonSize::Lg => (20.0, 24.0),
            ButtonSize::Icon | ButtonSize::Text => (0.0, 0.0),
        }
    }

    pub fn radius(self) -> f32 {
        match self {
            ButtonSize::Sm | ButtonSize::Icon => RADIUS_SM,
            _ => RADIUS_MD,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Surface {
    pub fill: Hsla,
    pub text: Hsla,
    pub border: Option<Hsla>,
    pub opacity: f32,
}

pub fn button_surface(
    variant: ButtonVariant,
    colors: Palette,
    hover: f32,
    disabled: bool,
) -> Surface {
    let transparent = opacity(colors.accent, 0.0);

    let mut surface = match variant {
        ButtonVariant::Default => Surface {
            fill: mix(colors.foreground, opacity(colors.foreground, 0.9), hover),
            text: colors.background,
            border: None,
            opacity: 1.0,
        },
        ButtonVariant::Background => Surface {
            fill: mix(colors.background, opacity(colors.background, 0.9), hover),
            text: colors.foreground,
            border: None,
            opacity: 1.0,
        },
        ButtonVariant::Destructive => Surface {
            fill: mix(colors.destructive, opacity(colors.destructive, 0.8), hover),
            text: colors.destructive_foreground,
            border: None,
            opacity: 1.0,
        },
        ButtonVariant::DestructiveForeground => Surface {
            fill: mix(colors.background, opacity(colors.destructive, 0.15), hover),
            text: colors.destructive,
            border: Some(colors.border),
            opacity: 1.0,
        },
        ButtonVariant::Caution => Surface {
            fill: mix(transparent, opacity(colors.caution, 0.1), hover),
            text: colors.caution,
            border: None,
            opacity: 1.0,
        },
        ButtonVariant::Outline => Surface {
            fill: mix(colors.background, colors.accent, hover),
            text: colors.foreground,
            border: Some(colors.border),
            opacity: 1.0,
        },
        ButtonVariant::Secondary => Surface {
            fill: colors.secondary,
            text: colors.secondary_foreground,
            border: Some(colors.secondary_border),
            opacity: 1.0,
        },
        ButtonVariant::Text => Surface {
            fill: transparent,
            text: colors.foreground,
            border: None,
            opacity: 1.0 - (1.0 - TEXT_HOVER_OPACITY) * hover,
        },
        ButtonVariant::Ghost => Surface {
            fill: mix(transparent, colors.accent, hover),
            text: colors.foreground,
            border: None,
            opacity: 1.0,
        },
        ButtonVariant::Link => Surface {
            fill: transparent,
            text: colors.primary,
            border: None,
            opacity: 1.0,
        },
    };

    if disabled {
        surface.opacity *= DISABLED_OPACITY;
    }
    surface
}

pub struct Button {
    id: SharedString,
    variant: ButtonVariant,
    size: ButtonSize,
    colors: Palette,
    hover: f32,
    disabled: bool,
    icon: Option<SharedString>,
    label: Option<SharedString>,
    trailing: Option<SharedString>,
    style: StyleRefinement,
}

impl Button {
    pub fn new(id: impl Into<SharedString>, colors: Palette) -> Self {
        Self {
            id: id.into(),
            variant: ButtonVariant::Default,
            size: ButtonSize::Default,
            colors,
            hover: 0.0,
            disabled: false,
            icon: None,
            label: None,
            trailing: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        if variant == ButtonVariant::Text {
            self.size = ButtonSize::Text;
        }
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn hover(mut self, progress: f32) -> Self {
        self.hover = progress.clamp(0.0, 1.0);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn icon(mut self, name: &str) -> Self {
        self.icon = Some(icon(name));
        self
    }

    pub fn trailing(mut self, name: &str) -> Self {
        self.trailing = Some(icon(name));
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn build(self) -> Stateful<Div> {
        let surface = button_surface(self.variant, self.colors, self.hover, self.disabled);
        let (py, px_pad) = self.size.padding();
        let text = opacity(surface.text, surface.opacity);

        let mut root = div()
            .id(self.id)
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap(px(BUTTON_GAP_PX))
            .text_size(rem(TEXT_SM))
            .font_weight(FontWeight::MEDIUM)
            .text_color(text)
            .bg(opacity(surface.fill, surface.opacity));

        if self.disabled {
            root = root.cursor_not_allowed();
        } else {
            root = root
                .cursor_pointer()
                .active(|style| style.opacity(PRESSED_OPACITY));
        }

        if self.variant != ButtonVariant::Text {
            root = root.rounded(rem(self.size.radius()));
        }

        if let Some(height) = self.size.height() {
            root = root.h(px(height));
            if self.size == ButtonSize::Icon {
                root = root.w(px(height));
            }
        }
        if px_pad > 0.0 {
            root = root.px(px(px_pad));
        }
        if py > 0.0 {
            root = root.py(px(py));
        }
        if let Some(border) = surface.border {
            root = root
                .border_1()
                .border_color(opacity(border, surface.opacity));
        }

        let glyph = |path: SharedString| {
            gpui::svg()
                .size(px(BUTTON_ICON_PX))
                .flex_shrink_0()
                .path(path)
                .text_color(text)
        };

        let root = root
            .when_some(self.icon, |this, path| this.child(glyph(path)))
            .when_some(self.label, |this, label| this.child(label))
            .when_some(self.trailing, |this, path| this.child(glyph(path)));

        let mut root = root;
        root.style().refine(&self.style);
        root
    }
}

impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

pub fn icon_button(
    id: impl Into<SharedString>,
    name: &str,
    colors: Palette,
    variant: ButtonVariant,
    hover: f32,
) -> Button {
    Button::new(id, colors)
        .variant(variant)
        .size(ButtonSize::Icon)
        .hover(hover)
        .icon(name)
}

pub fn separator_h(colors: Palette) -> Div {
    div().h(px(1.0)).w_full().flex_shrink_0().bg(colors.border)
}

pub fn separator_v(colors: Palette, height: f32) -> Div {
    div()
        .w(px(1.0))
        .h(px(height))
        .flex_shrink_0()
        .bg(colors.border)
}

pub const TOOLTIP_RADIUS: f32 = RADIUS_SM;
pub const TOOLTIP_PAD_Y_PX: f32 = 6.0;
pub const TOOLTIP_PAD_X_PX: f32 = 12.0;
pub const TOOLTIP_OFFSET_PX: f32 = 4.0;

pub fn tooltip(colors: Palette, label: impl Into<SharedString>, frame: OverlayFrame) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .whitespace_nowrap()
        .rounded(rem(TOOLTIP_RADIUS))
        .border_1()
        .border_color(colors.border)
        .bg(colors.popover)
        .text_color(colors.popover_foreground)
        .text_size(rem(TEXT_SM))
        .py(px(TOOLTIP_PAD_Y_PX))
        .px(px(TOOLTIP_PAD_X_PX))
        .shadow_md()
        .opacity(frame.opacity)
        .child(label.into())
}

pub fn tooltipped(
    control: impl IntoElement,
    colors: Palette,
    label: impl Into<SharedString>,
    frame: Option<OverlayFrame>,
    side: OverlaySide,
) -> Div {
    let mut root = div().relative().flex().flex_shrink_0().child(control);

    if let Some(frame) = frame {
        let gap = TOOLTIP_OFFSET_PX + frame.offset;
        let anchor = div().absolute().w(px(0.0)).h(px(0.0));
        let (corner, anchor) = match side {
            OverlaySide::Top => (gpui::Corner::BottomLeft, anchor.left_0().top(px(-gap))),
            OverlaySide::Bottom => (gpui::Corner::TopLeft, anchor.left_0().bottom(px(-gap))),
            OverlaySide::Left => (gpui::Corner::TopRight, anchor.top_0().left(px(-gap))),
            OverlaySide::Right => (gpui::Corner::TopLeft, anchor.top_0().right(px(-gap))),
        };

        root = root.child(
            anchor.child(
                gpui::deferred(
                    gpui::anchored()
                        .anchor(corner)
                        .snap_to_window_with_margin(px(8.0))
                        .child(tooltip(colors, label, frame)),
                )
                .with_priority(2),
            ),
        );
    }

    root
}

pub const MENU_MIN_WIDTH_PX: f32 = 128.0;
pub const MENU_PAD_PX: f32 = 4.0;
pub const MENU_ITEM_PAD_Y_PX: f32 = 6.0;
pub const MENU_ITEM_PAD_X_PX: f32 = 10.0;
pub const MENU_OFFSET_PX: f32 = 4.0;
pub const MENU_ITEM_TEXT_OPACITY: f32 = 0.85;

pub const MENU_ITEM_HEIGHT_PX: f32 = 31.0;

pub fn menu_action(
    id: impl Into<SharedString>,
    colors: Palette,
    label: impl Into<SharedString>,
    glyph: &str,
    highlighted: bool,
    destructive: bool,
) -> Stateful<Div> {
    let fill = if highlighted {
        if destructive {
            opacity(colors.destructive, 0.15)
        } else {
            colors.popover_hover
        }
    } else {
        opacity(colors.popover_hover, 0.0)
    };
    let text = if destructive {
        colors.destructive
    } else {
        opacity(colors.popover_foreground, MENU_ITEM_TEXT_OPACITY)
    };

    div()
        .id(id.into())
        .flex()
        .w_full()
        .h(px(MENU_ITEM_HEIGHT_PX))
        .flex_shrink_0()
        .items_center()
        .gap(px(BUTTON_GAP_PX))
        .rounded(rem(RADIUS_SM))
        .px(px(MENU_ITEM_PAD_X_PX))
        .cursor_pointer()
        .active(|style| style.opacity(PRESSED_OPACITY))
        .text_size(rem(TEXT_SM))
        .text_color(text)
        .bg(fill)
        .child(
            gpui::svg()
                .size(px(15.0))
                .flex_shrink_0()
                .path(icon(glyph))
                .text_color(text),
        )
        .child(label.into())
}

pub fn menu_item(
    id: impl Into<SharedString>,
    colors: Palette,
    label: impl Into<SharedString>,
    highlighted: bool,
    checked: bool,
) -> Stateful<Div> {
    let fill = if highlighted {
        colors.popover_hover
    } else {
        opacity(colors.popover_hover, 0.0)
    };

    div()
        .id(id.into())
        .flex()
        .w_full()
        .h(px(MENU_ITEM_HEIGHT_PX))
        .flex_shrink_0()
        .items_center()
        .gap(px(BUTTON_GAP_PX))
        .rounded(rem(RADIUS_SM))
        .px(px(MENU_ITEM_PAD_X_PX))
        .cursor_pointer()
        .active(|style| style.opacity(PRESSED_OPACITY))
        .text_size(rem(TEXT_SM))
        .text_color(opacity(colors.popover_foreground, MENU_ITEM_TEXT_OPACITY))
        .bg(fill)
        .child(
            div()
                .flex()
                .size(px(14.0))
                .flex_shrink_0()
                .when(checked, |this| {
                    this.child(
                        gpui::svg()
                            .size(px(14.0))
                            .path(icon("tick02"))
                            .text_color(colors.popover_foreground),
                    )
                }),
        )
        .child(label.into())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuPlacement {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
}

pub fn place_menu(
    frame: OverlayFrame,
    side: OverlaySide,
    natural: (f32, f32),
    anchor: (f32, f32),
) -> MenuPlacement {
    let (natural_w, natural_h) = natural;
    let (origin_x, origin_y) = side.origin();
    let width = natural_w * frame.scale;
    let height = natural_h * frame.scale;

    let slide_y = match side {
        OverlaySide::Bottom => frame.offset,
        OverlaySide::Top => -frame.offset,
        _ => 0.0,
    };
    let slide_x = match side {
        OverlaySide::Right => frame.offset,
        OverlaySide::Left => -frame.offset,
        _ => 0.0,
    };

    MenuPlacement {
        left: anchor.0 + (natural_w - width) * origin_x + slide_x,
        top: anchor.1 + (natural_h - height) * origin_y + slide_y,
        width,
        height,
        opacity: frame.opacity,
    }
}

pub fn place_anchored(
    frame: OverlayFrame,
    side: OverlaySide,
    natural: (f32, f32),
    anchor: (f32, f32),
) -> MenuPlacement {
    let slide_y = match side {
        OverlaySide::Bottom => frame.offset,
        OverlaySide::Top => -frame.offset,
        _ => 0.0,
    };
    let slide_x = match side {
        OverlaySide::Right => frame.offset,
        OverlaySide::Left => -frame.offset,
        _ => 0.0,
    };

    MenuPlacement {
        left: anchor.0 + slide_x,
        top: anchor.1 + slide_y,
        width: natural.0 * frame.scale,
        height: natural.1 * frame.scale,
        opacity: frame.opacity,
    }
}

pub fn menu_surface(colors: Palette, placement: MenuPlacement) -> Div {
    div()
        .w(px(placement.width))
        .h(px(placement.height))
        .flex()
        .flex_col()
        .overflow_hidden()
        .p(px(MENU_PAD_PX))
        .rounded(rem(RADIUS_MD))
        .border_1()
        .border_color(colors.border)
        .bg(colors.popover)
        .text_color(colors.popover_foreground)
        .shadow_lg()
        .opacity(placement.opacity)
}

pub fn overlay_layer(
    corner: gpui::Corner,
    placement: MenuPlacement,
    surface: impl IntoElement,
) -> impl IntoElement {
    div()
        .absolute()
        .left(px(placement.left))
        .top(px(placement.top))
        .w(px(0.0))
        .h(px(0.0))
        .child(
            gpui::deferred(
                gpui::anchored()
                    .anchor(corner)
                    .snap_to_window_with_margin(px(8.0))
                    .child(surface),
            )
            .with_priority(1),
        )
}

pub fn overlay_root() -> Div {
    div()
        .absolute()
        .left(px(0.0))
        .top(px(0.0))
        .w(px(0.0))
        .h(px(0.0))
}

pub fn overlay_backdrop(id: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id.into())
        .absolute()
        .left(px(-8000.0))
        .top(px(-8000.0))
        .w(px(16000.0))
        .h(px(16000.0))
}

pub fn aspect_fit(element: Div, ratio: f32) -> Div {
    let mut element = element.w_full().h_full();
    let style = element.style();
    style.aspect_ratio = Some(ratio);
    style.max_size.width = Some(gpui::relative(1.0).into());
    style.max_size.height = Some(gpui::relative(1.0).into());
    element
}

pub fn menu_natural_height(items: usize, item_height: f32) -> f32 {
    items as f32 * item_height + MENU_PAD_PX * 2.0 + 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn dark() -> Palette {
        Theme::dark().panel
    }

    #[test]
    fn sizes_match_the_web_button_matrix() {
        assert_eq!(ButtonSize::Default.height(), Some(36.0));
        assert_eq!(ButtonSize::Sm.height(), Some(28.0));
        assert_eq!(ButtonSize::Lg.height(), Some(40.0));
        assert_eq!(ButtonSize::Icon.height(), Some(28.0));
        assert_eq!(ButtonSize::Text.height(), None);
        assert_eq!(ButtonSize::Default.padding(), (8.0, 16.0));
        assert_eq!(ButtonSize::Sm.padding(), (4.0, 10.0));
        assert_eq!(ButtonSize::Lg.padding(), (20.0, 24.0));
    }

    #[test]
    fn small_and_icon_sizes_use_the_tight_radius() {
        assert_eq!(ButtonSize::Sm.radius(), RADIUS_SM);
        assert_eq!(ButtonSize::Icon.radius(), RADIUS_SM);
        assert_eq!(ButtonSize::Default.radius(), RADIUS_MD);
    }

    #[test]
    fn the_text_variant_forces_the_text_size() {
        let button = Button::new("x", dark()).variant(ButtonVariant::Text);
        assert_eq!(button.size, ButtonSize::Text);
    }

    #[test]
    fn outline_fills_with_accent_on_hover() {
        let colors = dark();
        let rest = button_surface(ButtonVariant::Outline, colors, 0.0, false);
        let hovered = button_surface(ButtonVariant::Outline, colors, 1.0, false);
        assert_eq!(rest.fill, colors.background);
        assert_eq!(hovered.fill, colors.accent);
        assert_eq!(rest.border, Some(colors.border));
    }

    #[test]
    fn the_secondary_variant_has_no_hover_change() {
        let colors = dark();
        let rest = button_surface(ButtonVariant::Secondary, colors, 0.0, false);
        let hovered = button_surface(ButtonVariant::Secondary, colors, 1.0, false);
        assert_eq!(rest.fill, hovered.fill);
        assert_eq!(rest.border, Some(colors.secondary_border));
    }

    #[test]
    fn the_text_variant_fades_rather_than_fills() {
        let colors = dark();
        let rest = button_surface(ButtonVariant::Text, colors, 0.0, false);
        let hovered = button_surface(ButtonVariant::Text, colors, 1.0, false);
        assert_eq!(rest.opacity, 1.0);
        assert_eq!(hovered.opacity, TEXT_HOVER_OPACITY);
        assert_eq!(rest.fill.a, 0.0);
    }

    #[test]
    fn ghost_fades_in_from_a_transparent_accent() {
        let colors = dark();
        let rest = button_surface(ButtonVariant::Ghost, colors, 0.0, false);
        assert_eq!(rest.fill.a, 0.0);
        let half = button_surface(ButtonVariant::Ghost, colors, 0.5, false);
        assert!(half.fill.a > 0.0 && half.fill.a < colors.accent.a);
    }

    #[test]
    fn disabled_halves_the_opacity_of_every_variant() {
        let colors = dark();
        for variant in [
            ButtonVariant::Default,
            ButtonVariant::Outline,
            ButtonVariant::Ghost,
            ButtonVariant::Text,
            ButtonVariant::Secondary,
        ] {
            let on = button_surface(variant, colors, 0.0, false);
            let off = button_surface(variant, colors, 0.0, true);
            assert!((off.opacity - on.opacity * 0.5).abs() < 1e-6, "{variant:?}");
        }
    }

    #[test]
    fn menu_geometry_matches_the_dropdown_spec() {
        assert_eq!(MENU_MIN_WIDTH_PX, 128.0);
        assert_eq!(MENU_PAD_PX, 4.0);
        assert_eq!((MENU_ITEM_PAD_Y_PX, MENU_ITEM_PAD_X_PX), (6.0, 10.0));
        assert_eq!(MENU_OFFSET_PX, 4.0);
        assert_eq!(MENU_ITEM_TEXT_OPACITY, 0.85);
    }

    #[test]
    fn a_bottom_menu_grows_downward_from_its_top_edge() {
        let frame = OverlayFrame {
            opacity: 0.5,
            scale: 0.95,
            offset: 4.0,
            visible: true,
        };
        let placed = place_menu(frame, OverlaySide::Bottom, (200.0, 100.0), (10.0, 20.0));
        assert!((placed.width - 190.0).abs() < 1e-4);
        assert!((placed.height - 95.0).abs() < 1e-4);
        assert!((placed.left - 15.0).abs() < 1e-4);
        assert!((placed.top - 24.0).abs() < 1e-4);
    }

    #[test]
    fn a_settled_menu_sits_exactly_on_its_anchor() {
        let frame = OverlayFrame {
            opacity: 1.0,
            scale: 1.0,
            offset: 0.0,
            visible: true,
        };
        let placed = place_menu(frame, OverlaySide::Bottom, (200.0, 100.0), (10.0, 20.0));
        assert_eq!((placed.left, placed.top), (10.0, 20.0));
        assert_eq!((placed.width, placed.height), (200.0, 100.0));
    }

    #[test]
    fn menu_height_accounts_for_padding_and_border() {
        assert_eq!(menu_natural_height(3, 30.0), 3.0 * 30.0 + 8.0 + 2.0);
    }

    #[test]
    fn tooltip_geometry_matches_the_spec() {
        assert_eq!((TOOLTIP_PAD_Y_PX, TOOLTIP_PAD_X_PX), (6.0, 12.0));
        assert_eq!(TOOLTIP_OFFSET_PX, 4.0);
        assert_eq!(TOOLTIP_RADIUS, RADIUS_SM);
    }

    #[test]
    fn base_button_metrics_match_the_web() {
        assert_eq!(BUTTON_GAP_PX, 8.0);
        assert_eq!(BUTTON_ICON_PX, 16.0);
    }
}
