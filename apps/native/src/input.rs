use gpui::{div, prelude::*, px, App, FocusHandle, KeyDownEvent, SharedString, Window};

use crate::theme::{opacity, rem, Palette, RADIUS_MD, TEXT_SM};

pub enum TextEvent {
    Changed,
    Submit,
    Cancel,
    Ignored,

    Copy(String),

    Cut(String),

    Paste,
}

#[derive(Default, Debug, Clone)]
pub struct TextBuffer {
    pub text: String,
    pub cursor: usize,

    pub anchor: Option<usize>,
}

impl TextBuffer {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            cursor: text.chars().count(),
            text,
            anchor: None,
        }
    }

    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        if anchor == self.cursor {
            return None;
        }
        Some((anchor.min(self.cursor), anchor.max(self.cursor)))
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection()?;
        Some(self.text[self.byte_offset(start)..self.byte_offset(end)].to_string())
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.text.chars().count();
    }

    pub fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection() else {
            self.anchor = None;
            return false;
        };
        let range = self.byte_offset(start)..self.byte_offset(end);
        self.text.replace_range(range, "");
        self.cursor = start;
        self.anchor = None;
        true
    }

    pub fn insert(&mut self, text: &str) {
        self.delete_selection();
        let at = self.byte_offset(self.cursor);
        self.text.insert_str(at, text);
        self.cursor += text.chars().count();
    }

    fn move_cursor(&mut self, to: usize, extend: bool) {
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
        self.cursor = to;
    }

    pub fn set(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.chars().count();
        self.anchor = None;
    }

    pub fn byte_offset(&self, cursor: usize) -> usize {
        self.text
            .char_indices()
            .nth(cursor)
            .map(|(offset, _)| offset)
            .unwrap_or(self.text.len())
    }

    pub fn key_down(&mut self, event: &KeyDownEvent) -> TextEvent {
        self.key_down_in(event, false)
    }

    pub fn key_down_in(&mut self, event: &KeyDownEvent, multiline: bool) -> TextEvent {
        let keystroke = &event.keystroke;
        let count = self.text.chars().count();
        self.cursor = self.cursor.min(count);

        let control = keystroke.modifiers.control || keystroke.modifiers.platform;
        let extend = keystroke.modifiers.shift;

        match keystroke.key.as_str() {
            "enter" => {
                if !multiline {
                    return TextEvent::Submit;
                }
                self.insert("\n");
                return TextEvent::Changed;
            }
            "escape" => return TextEvent::Cancel,
            "backspace" => {
                if self.delete_selection() {
                    return TextEvent::Changed;
                }
                if self.cursor == 0 {
                    return TextEvent::Ignored;
                }
                let start = self.byte_offset(self.cursor - 1);
                let end = self.byte_offset(self.cursor);
                self.text.replace_range(start..end, "");
                self.cursor -= 1;
                return TextEvent::Changed;
            }
            "delete" => {
                if self.delete_selection() {
                    return TextEvent::Changed;
                }
                if self.cursor >= count {
                    return TextEvent::Ignored;
                }
                let start = self.byte_offset(self.cursor);
                let end = self.byte_offset(self.cursor + 1);
                self.text.replace_range(start..end, "");
                return TextEvent::Changed;
            }
            "left" => {
                let to = self.cursor.saturating_sub(1);
                self.move_cursor(to, extend);
                return TextEvent::Changed;
            }
            "right" => {
                let to = (self.cursor + 1).min(count);
                self.move_cursor(to, extend);
                return TextEvent::Changed;
            }
            "home" => {
                self.move_cursor(0, extend);
                return TextEvent::Changed;
            }
            "end" => {
                self.move_cursor(count, extend);
                return TextEvent::Changed;
            }

            "a" if control => {
                self.select_all();
                return TextEvent::Changed;
            }
            "c" if control => {
                return match self.selected_text() {
                    Some(text) => TextEvent::Copy(text),
                    None => TextEvent::Ignored,
                };
            }
            "x" if control => {
                return match self.selected_text() {
                    Some(text) => {
                        self.delete_selection();
                        TextEvent::Cut(text)
                    }
                    None => TextEvent::Ignored,
                };
            }
            "v" if control => return TextEvent::Paste,
            _ => {}
        }

        if keystroke.modifiers.control || keystroke.modifiers.alt || keystroke.modifiers.platform {
            return TextEvent::Ignored;
        }

        let typed = match keystroke.key_char.as_deref() {
            Some(typed) => typed,
            None if keystroke.key == "space" => " ",
            None => return TextEvent::Ignored,
        };
        if typed.is_empty() || typed.chars().any(char::is_control) {
            return TextEvent::Ignored;
        }

        self.insert(typed);
        TextEvent::Changed
    }
}

pub fn key_down_with_clipboard(
    buffer: &mut TextBuffer,
    event: &KeyDownEvent,
    multiline: bool,
    cx: &mut App,
) -> TextEvent {
    match buffer.key_down_in(event, multiline) {
        TextEvent::Copy(text) | TextEvent::Cut(text) => {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            TextEvent::Changed
        }
        TextEvent::Paste => {
            let pasted = cx.read_from_clipboard().and_then(|item| item.text());
            match pasted {
                Some(text) if !text.is_empty() => {
                    let text = if multiline {
                        text
                    } else {
                        text.replace(['\n', '\r'], " ")
                    };
                    buffer.insert(&text);
                    TextEvent::Changed
                }
                _ => TextEvent::Ignored,
            }
        }
        TextEvent::Changed => TextEvent::Changed,
        TextEvent::Submit => TextEvent::Submit,
        TextEvent::Cancel => TextEvent::Cancel,
        TextEvent::Ignored => TextEvent::Ignored,
    }
}

const FIELDS_REMEMBERED: usize = 512;

thread_local! {
    static TYPING_TARGETS: std::cell::RefCell<std::collections::VecDeque<FocusHandle>> =
        const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}

fn remember_field(handle: &FocusHandle) {
    TYPING_TARGETS.with(|known| {
        let Ok(mut known) = known.try_borrow_mut() else {
            return;
        };
        if known.iter().any(|known| known == handle) {
            return;
        }
        if known.len() >= FIELDS_REMEMBERED {
            known.pop_front();
        }
        known.push_back(handle.clone());
    });
}

pub fn is_typing(window: &Window, cx: &App) -> bool {
    let Some(focused) = window.focused(cx) else {
        return false;
    };
    TYPING_TARGETS.with(|known| {
        known
            .try_borrow()
            .map(|known| known.iter().any(|handle| *handle == focused))
            .unwrap_or(true)
    })
}

pub struct TextField {
    pub buffer: TextBuffer,
    pub focus: FocusHandle,
}

impl TextField {
    pub fn new(cx: &mut App, text: impl Into<String>) -> Self {
        let focus = cx.focus_handle();
        remember_field(&focus);
        Self {
            buffer: TextBuffer::new(text),
            focus,
        }
    }

    pub fn text(&self) -> &str {
        &self.buffer.text
    }
}

pub struct FieldStyle {
    pub height: f32,
    pub placeholder: SharedString,
    pub leading: Option<SharedString>,

    pub masked: bool,

    pub trailing: Option<gpui::AnyElement>,

    pub multiline: bool,
}

impl Default for FieldStyle {
    fn default() -> Self {
        Self {
            height: 32.0,
            placeholder: SharedString::default(),
            leading: None,
            masked: false,
            trailing: None,
            multiline: false,
        }
    }
}

pub const MASK_CHARACTER: char = '\u{2022}';

pub fn masked_text(text: &str) -> String {
    std::iter::repeat_n(MASK_CHARACTER, text.chars().count()).collect()
}

fn multiline_rows(
    before: &str,
    after: &str,
    caret: gpui::Div,
    selection: Option<(String, String, String)>,
    highlight: gpui::Hsla,
) -> Vec<gpui::Div> {
    if let Some((head, selected, tail)) = selection {
        return vec![div()
            .flex()
            .flex_wrap()
            .w_full()
            .child(div().max_w_full().child(head))
            .child(
                div()
                    .max_w_full()
                    .rounded(px(2.0))
                    .bg(highlight)
                    .child(selected),
            )
            .child(div().max_w_full().child(tail))];
    }

    let mut rows: Vec<gpui::Div> = Vec::new();
    let head: Vec<&str> = before.split('\n').collect();
    let tail: Vec<&str> = after.split('\n').collect();

    for line in head.iter().take(head.len().saturating_sub(1)) {
        rows.push(div().w_full().child(line.to_string()));
    }

    let caret_before = head.last().copied().unwrap_or_default().to_string();
    let caret_after = tail.first().copied().unwrap_or_default().to_string();
    rows.push(
        div()
            .flex()
            .flex_wrap()
            .w_full()
            .child(div().max_w_full().child(caret_before))
            .child(caret)
            .child(div().max_w_full().child(caret_after)),
    );

    for line in tail.iter().skip(1) {
        rows.push(div().w_full().child(line.to_string()));
    }

    rows
}

pub fn text_field(
    id: impl Into<SharedString>,
    field: &TextField,
    colors: Palette,
    style: FieldStyle,
    window: &Window,
) -> gpui::Stateful<gpui::Div> {
    let focused = field.focus.is_focused(window);
    let trailing = style.trailing;
    let buffer = &field.buffer;
    let (before, after) = buffer.text.split_at(buffer.byte_offset(buffer.cursor));
    let (before, after) = if style.masked {
        (masked_text(before), masked_text(after))
    } else {
        (before.to_string(), after.to_string())
    };

    let handle = field.focus.clone();

    let caret = div()
        .w(px(1.0))
        .h(rem(0.95))
        .flex_shrink_0()
        .bg(if focused {
            colors.foreground
        } else {
            opacity(colors.foreground, 0.0)
        });

    let multiline = style.multiline;

    let selection = (!style.masked)
        .then(|| buffer.selection())
        .flatten()
        .map(|(start, end)| {
            let (head, rest) = buffer.text.split_at(buffer.byte_offset(start));
            let taken = buffer.byte_offset(end) - buffer.byte_offset(start);
            let (selected, tail) = rest.split_at(taken);
            (head.to_string(), selected.to_string(), tail.to_string())
        });
    let single_line_selection = selection.clone();

    div()
        .id(id.into())
        .track_focus(&field.focus)
        .on_mouse_down(gpui::MouseButton::Left, move |_, window: &mut Window, _| {
            window.focus(&handle)
        })
        .flex()
        .h(px(style.height))
        .w_full()
        .map(|this| {
            if multiline {
                this.items_start().py(px(8.0))
            } else {
                this.items_center()
            }
        })
        .gap(px(6.0))
        .px(px(10.0))
        .rounded(rem(RADIUS_MD))
        .border_1()
        .border_color(if focused { colors.ring } else { colors.border })
        .bg(colors.input)
        .text_size(rem(TEXT_SM))
        .text_color(colors.foreground)
        .cursor_text()
        .overflow_hidden()
        .when_some(style.leading, |this, path| {
            this.child(
                gpui::svg()
                    .size(px(16.0))
                    .flex_shrink_0()
                    .path(path)
                    .text_color(colors.muted_foreground),
            )
        })
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .map(|this| {
                    if multiline {
                        this.flex_col().items_start().gap(px(1.0))
                    } else {
                        this.items_center()
                    }
                })
                .when(buffer.text.is_empty() && !focused, |this| {
                    this.child(
                        div()
                            .text_color(colors.muted_foreground)
                            .child(style.placeholder.clone()),
                    )
                })
                .when(!buffer.text.is_empty() || focused, |this| {
                    if multiline {
                        this.children(multiline_rows(
                            &before,
                            &after,
                            caret,
                            selection,
                            opacity(colors.primary, 0.35),
                        ))
                    } else if let Some((head, selected, tail)) = single_line_selection {
                        this.child(div().flex_shrink_0().child(head))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .rounded(px(2.0))
                                    .bg(opacity(colors.primary, 0.35))
                                    .child(selected),
                            )
                            .child(div().flex_shrink_0().child(tail))
                    } else {
                        this.child(div().flex_shrink_0().child(before))
                            .child(caret)
                            .child(div().flex_shrink_0().child(after))
                    }
                }),
        )
        .when_some(trailing, |this, control| {
            this.child(div().flex_shrink_0().child(control))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Keystroke, Modifiers};

    fn press_with(key: &str, key_char: Option<&str>, control: bool, shift: bool) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: Keystroke {
                modifiers: Modifiers {
                    control,
                    shift,
                    ..Modifiers::default()
                },
                key: key.to_string(),
                key_char: key_char.map(str::to_string),
            },
            is_held: false,
        }
    }

    fn press(key: &str, key_char: Option<&str>) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: Keystroke {
                modifiers: Modifiers::default(),
                key: key.to_string(),
                key_char: key_char.map(str::to_string),
            },
            is_held: false,
        }
    }

    #[test]
    fn select_all_selects_rather_than_deletes() {
        let mut buffer = TextBuffer::new("hello");
        buffer.key_down(&press_with("a", None, true, false));
        assert_eq!(buffer.text, "hello", "Ctrl+A must not empty the field");
        assert_eq!(buffer.selection(), Some((0, 5)));
        assert_eq!(buffer.selected_text().as_deref(), Some("hello"));
    }

    #[test]
    fn typing_over_a_selection_replaces_it() {
        let mut buffer = TextBuffer::new("hello");
        buffer.select_all();
        buffer.key_down(&press("x", Some("x")));
        assert_eq!(buffer.text, "x");
        assert_eq!(buffer.selection(), None);
    }

    #[test]
    fn backspace_over_a_selection_removes_the_selection_only() {
        let mut buffer = TextBuffer::new("hello");
        buffer.anchor = Some(1);
        buffer.cursor = 4;
        buffer.key_down(&press("backspace", None));
        assert_eq!(buffer.text, "ho");
        assert_eq!(buffer.cursor, 1);
    }

    #[test]
    fn shift_arrows_grow_a_selection_and_plain_ones_drop_it() {
        let mut buffer = TextBuffer::new("hello");
        buffer.cursor = 0;
        buffer.key_down(&press_with("right", None, false, true));
        buffer.key_down(&press_with("right", None, false, true));
        assert_eq!(buffer.selection(), Some((0, 2)));
        buffer.key_down(&press("right", None));
        assert_eq!(buffer.selection(), None);
    }

    #[test]
    fn copy_and_cut_hand_over_the_selected_text() {
        let mut buffer = TextBuffer::new("hello");
        buffer.select_all();
        assert!(matches!(
            buffer.key_down(&press_with("c", None, true, false)),
            TextEvent::Copy(text) if text == "hello"
        ));
        assert_eq!(buffer.text, "hello", "copying leaves the text alone");
        assert!(matches!(
            buffer.key_down(&press_with("x", None, true, false)),
            TextEvent::Cut(text) if text == "hello"
        ));
        assert_eq!(buffer.text, "", "cutting takes it");
    }

    #[test]
    fn a_masked_field_shows_one_dot_per_character_and_keeps_the_real_text() {
        let mut buffer = TextBuffer::new("");
        for key in ["h", "u", "n", "t"] {
            buffer.key_down(&press(key, Some(key)));
        }
        assert_eq!(
            buffer.text, "hunt",
            "the buffer is what gets sent to Google"
        );
        assert_eq!(masked_text(&buffer.text), "••••");

        let (before, after) = buffer.text.split_at(buffer.byte_offset(2));
        assert_eq!(masked_text(before).chars().count(), 2);
        assert_eq!(masked_text(after).chars().count(), 2);
    }

    #[test]
    fn masking_counts_characters_rather_than_bytes() {
        assert_eq!(masked_text("пароль").chars().count(), 6);
        assert_eq!(masked_text("héllo").chars().count(), 5);
        assert_eq!(masked_text(""), "");
    }

    #[test]
    fn a_field_is_unmasked_and_bare_unless_it_asks_otherwise() {
        let style = FieldStyle::default();
        assert!(!style.masked);
        assert!(style.trailing.is_none());
        assert!(style.leading.is_none());
    }

    #[test]
    fn typing_inserts_at_the_caret() {
        let mut buffer = TextBuffer {
            anchor: None,
            text: String::from("ab"),
            cursor: 1,
        };
        buffer.key_down(&press("x", Some("x")));
        assert_eq!(buffer.text, "axb");
        assert_eq!(buffer.cursor, 2);
    }

    #[test]
    fn backspace_removes_a_whole_multibyte_character() {
        let mut buffer = TextBuffer::new("проект");
        buffer.key_down(&press("backspace", None));
        assert_eq!(buffer.text, "проек");
        assert_eq!(buffer.cursor, 5);
    }

    #[test]
    fn arrows_clamp_to_the_text_bounds() {
        let mut buffer = TextBuffer::new("ab");
        buffer.key_down(&press("right", None));
        assert_eq!(buffer.cursor, 2);
        buffer.key_down(&press("home", None));
        assert_eq!(buffer.cursor, 0);
        buffer.key_down(&press("left", None));
        assert_eq!(buffer.cursor, 0);
    }

    #[test]
    fn enter_and_escape_are_reported_rather_than_typed() {
        let mut buffer = TextBuffer::new("x");
        assert!(matches!(
            buffer.key_down(&press("enter", None)),
            TextEvent::Submit
        ));
        assert!(matches!(
            buffer.key_down(&press("escape", None)),
            TextEvent::Cancel
        ));
        assert_eq!(buffer.text, "x");
    }

    #[test]
    fn the_space_bar_types_a_space_without_a_key_char() {
        let mut buffer = TextBuffer::new("Demo");
        buffer.key_down(&press("space", None));
        assert_eq!(buffer.text, "Demo ");
    }

    #[test]
    fn control_chords_do_not_type_their_letter() {
        let mut buffer = TextBuffer::new("hello");
        let mut event = press("c", Some("c"));
        event.keystroke.modifiers.control = true;
        assert!(matches!(buffer.key_down(&event), TextEvent::Ignored));
        assert_eq!(buffer.text, "hello");
    }
}
