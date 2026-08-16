//! Kitchen chat: the player orders, the chef plates an optimal build.

use std::path::Path;

use nexus::imgui::{ChildWindow, StyleColor, Ui};
use serde::{Deserialize, Serialize};

use crate::chat_links::ChatChip;
use crate::ui::theme;

/// State for the kitchen chat bar.
#[derive(Default)]
pub struct ChatBarState {
    pub input: String,
    pub history: Vec<ChatMessage>,
    pub waiting: bool,
    pub copied_code: Option<String>,
    pub copied_frames: u32,
    pub scroll_to_end: bool,
    pub dirty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub from_user: bool,
    pub text: String,
    pub chips: Vec<ChatChip>,
}

/// Maximum chat history entries retained. Beyond this, the oldest entries are
/// dropped on append so a long-running session can't grow the Vec without
/// bound. The plate scrolls; reserved height stays fixed.
const CHAT_HISTORY_CAP: usize = 100;

fn trim_history(history: &mut Vec<ChatMessage>) {
    if history.len() > CHAT_HISTORY_CAP {
        let drop = history.len() - CHAT_HISTORY_CAP;
        history.drain(..drop);
    }
}

const HEADER_H: f32 = 20.0;
const HISTORY_H: f32 = 128.0;
const COOKING_H: f32 = 18.0;
const INPUT_H: f32 = 28.0;
const PAD: f32 = 6.0;

/// Vertical space this bar will consume. Callers reserve this before the
/// scroll child so the input is not drawn on top of results. Height is
/// independent of history length — the plate scrolls instead.
pub fn reserved_height(state: &ChatBarState) -> f32 {
    let cooking = if state.waiting { COOKING_H } else { 0.0 };
    PAD + HEADER_H + HISTORY_H + cooking + INPUT_H
}

/// Render the kitchen chat bar at the bottom of the build view.
/// `cooking` is the live plating line (tool names) while the chef is working.
/// Returns Some(message) if the user submitted an order.
pub fn render_chat_bar(ui: &Ui, state: &mut ChatBarState, cooking: Option<&str>) -> Option<String> {
    let mut submitted = None;

    if state.copied_frames > 0 {
        state.copied_frames = state.copied_frames.saturating_sub(1);
        if state.copied_frames == 0 {
            state.copied_code = None;
        }
    }

    ui.spacing();
    ui.text_colored(theme::GOLD, "Kitchen");
    ui.same_line_with_spacing(0.0, 8.0);
    ui.text_colored(theme::MUTED, "customer \u{00b7} chef");
    if !state.history.is_empty() && !state.waiting {
        ui.same_line_with_spacing(0.0, 12.0);
        if ui.small_button("Clear##kitchen") {
            state.history.clear();
            state.copied_code = None;
            state.copied_frames = 0;
            state.dirty = true;
        }
    }

    let _child_bg = ui.push_style_color(StyleColor::ChildBg, theme::PLATE);
    ChildWindow::new("##kitchen_scroll")
        .size([0.0, HISTORY_H])
        .build(ui, || {
            if state.history.is_empty() && !state.waiting {
                theme::wrapped(
                    ui,
                    theme::MUTED,
                    "The kitchen is open. Paste a GW2 chat link or tell the chef what you want on the plate. Click a chip to copy it into game chat.",
                );
                return;
            }
            let n = state.history.len();
            for i in 0..n {
                let from_user = state.history[i].from_user;
                let (who, color) = if from_user {
                    ("You", theme::CURRENT)
                } else {
                    ("Chef", theme::GOLD)
                };
                ui.text_colored(color, who);
                let text = state.history[i].text.clone();
                theme::wrapped(ui, theme::CREAM, &text);
                render_chips(ui, state, i);
                ui.dummy([0.0, 4.0]);
            }
            if state.scroll_to_end {
                ui.set_scroll_here_y();
                state.scroll_to_end = false;
            }
        });
    drop(_child_bg);

    if state.waiting {
        let line = cooking
            .filter(|s| !s.is_empty())
            .unwrap_or("Chef is plating\u{2026}");
        theme::wrapped(ui, theme::GOLD, line);
    }

    let avail_width = ui.content_region_avail()[0];
    let button_width = 64.0;
    ui.set_next_item_width((avail_width - button_width - 10.0).max(40.0));
    if state.waiting {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        let mut dummy = String::new();
        ui.input_text("##chat_input", &mut dummy)
            .read_only(true)
            .build();
        style.pop();
        ui.same_line();
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        theme::gold_button_sized(ui, "Order", [button_width, 0.0]);
        style.pop();
        return submitted;
    }

    let enter_pressed = ui
        .input_text("##chat_input", &mut state.input)
        .hint("What shall we plate?")
        .enter_returns_true(true)
        .build();

    ui.same_line();

    let can_send = !state.input.is_empty();
    let send_clicked = theme::gold_button_sized(ui, "Order", [button_width, 0.0]) && can_send;

    if (enter_pressed || send_clicked) && can_send {
        let msg = state.input.trim().to_string();
        if !msg.is_empty() {
            state.history.push(ChatMessage {
                from_user: true,
                text: msg.clone(),
                chips: Vec::new(),
            });
            trim_history(&mut state.history);
            state.input.clear();
            state.scroll_to_end = true;
            state.dirty = true;
            submitted = Some(msg);
        }
    }

    submitted
}

fn render_chips(ui: &Ui, state: &mut ChatBarState, msg_i: usize) {
    let n = state.history[msg_i].chips.len();
    if n == 0 {
        return;
    }
    let avail = ui.content_region_avail()[0];
    let mut row_x = 0.0;
    for chip_i in 0..n {
        let label = if state.copied_code.as_deref()
            == Some(state.history[msg_i].chips[chip_i].code.as_str())
            && state.copied_frames > 0
        {
            "Copied".to_string()
        } else {
            state.history[msg_i].chips[chip_i].label.clone()
        };
        let pill_w = ui.calc_text_size(&label)[0] + 20.0;
        if chip_i > 0 {
            if row_x + pill_w + 4.0 > avail {
                row_x = 0.0;
            } else {
                ui.same_line_with_spacing(0.0, 4.0);
            }
        }
        let id = format!("##kchip{msg_i}_{chip_i}");
        let selected = state.copied_code.as_deref()
            == Some(state.history[msg_i].chips[chip_i].code.as_str())
            && state.copied_frames > 0;
        if theme::pill(ui, &label, selected, &id) {
            let code = state.history[msg_i].chips[chip_i].code.clone();
            if crate::clipboard::copy_text(&code) {
                state.copied_code = Some(code);
                state.copied_frames = 120;
            }
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Copy to Guild Wars 2 chat");
        }
        row_x += pill_w + 4.0;
    }
}

/// Add a chef reply with no serving chips (errors, timeout).
pub fn add_ai_response(state: &mut ChatBarState, text: String) {
    add_plated_response(state, text, Vec::new());
}

/// Add a chef reply and the plated serving tray.
pub fn add_plated_response(state: &mut ChatBarState, text: String, chips: Vec<ChatChip>) {
    state.waiting = false;
    // Cap for the plate, not the suggestion panel. Char-safe (no UTF-8 panic).
    let display = if text.chars().count() > 600 {
        let truncated: String = text.chars().take(600).collect();
        format!("{}...", truncated)
    } else {
        text
    };
    state.history.push(ChatMessage {
        from_user: false,
        text: display,
        chips,
    });
    trim_history(&mut state.history);
    state.scroll_to_end = true;
    state.dirty = true;
}

/// Attach inbound chips to the latest customer order.
pub fn attach_order_chips(state: &mut ChatBarState, display: String, chips: Vec<ChatChip>) {
    if let Some(last) = state.history.last_mut() {
        if last.from_user {
            last.text = display;
            last.chips = chips;
            state.dirty = true;
        }
    }
}

pub fn load_history(addon_dir: &Path) -> Vec<ChatMessage> {
    let path = addon_dir.join("kitchen.json");
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let Ok(mut hist) = serde_json::from_slice::<Vec<ChatMessage>>(&bytes) else {
        return Vec::new();
    };
    if hist.len() > CHAT_HISTORY_CAP {
        let drop = hist.len() - CHAT_HISTORY_CAP;
        hist.drain(..drop);
    }
    hist
}

pub fn save_history(addon_dir: &Path, history: &[ChatMessage]) {
    let path = addon_dir.join("kitchen.json");
    let Ok(json) = serde_json::to_vec(history) else {
        return;
    };
    let tmp = addon_dir.join("kitchen.json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_links::{encode_item, ChatChip, LinkKind};

    #[test]
    fn reserved_height_is_stable_with_history() {
        let mut state = ChatBarState::default();
        let empty = reserved_height(&state);
        for i in 0..8 {
            state.history.push(ChatMessage {
                from_user: i % 2 == 0,
                text: "a".into(),
                chips: Vec::new(),
            });
        }
        assert_eq!(
            reserved_height(&state),
            empty,
            "the plate scrolls; footer height must not grow with history"
        );
    }

    #[test]
    fn reserved_height_includes_waiting_line() {
        let mut state = ChatBarState::default();
        let idle = reserved_height(&state);
        state.waiting = true;
        assert!(reserved_height(&state) > idle);
    }

    #[test]
    fn add_ai_response_caps_utf8_without_panic() {
        let mut state = ChatBarState::default();
        state.waiting = true;
        let long: String = "é".repeat(700);
        add_ai_response(&mut state, long);
        assert!(!state.waiting);
        assert_eq!(state.history.len(), 1);
        assert!(state.history[0].text.chars().count() <= 604);
        assert!(state.history[0].chips.is_empty());
        assert!(state.scroll_to_end);
    }

    #[test]
    fn add_plated_response_keeps_chips() {
        let mut state = ChatBarState::default();
        state.waiting = true;
        add_plated_response(
            &mut state,
            "Tonight: Scholar.".into(),
            vec![ChatChip {
                kind: LinkKind::Item,
                label: "Rune of the Scholar".into(),
                code: encode_item(24836),
            }],
        );
        assert_eq!(state.history[0].chips.len(), 1);
        assert_eq!(state.history[0].chips[0].code, "[&AgEEYQAA]");
    }

    #[test]
    fn attach_order_chips_updates_last_customer_line() {
        let mut state = ChatBarState::default();
        state.history.push(ChatMessage {
            from_user: true,
            text: "[&AgEEYQAA]".into(),
            chips: Vec::new(),
        });
        attach_order_chips(
            &mut state,
            "Item #24836".into(),
            vec![ChatChip {
                kind: LinkKind::Item,
                label: "Item #24836".into(),
                code: encode_item(24836),
            }],
        );
        assert_eq!(state.history[0].text, "Item #24836");
        assert_eq!(state.history[0].chips.len(), 1);
    }

    #[test]
    fn kitchen_history_roundtrips_on_disk() {
        let dir = std::env::temp_dir().join(format!(
            "gw2_kitchen_hist_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let history = vec![ChatMessage {
            from_user: true,
            text: "plate this".into(),
            chips: vec![ChatChip {
                kind: LinkKind::Item,
                label: "Scholar".into(),
                code: encode_item(24836),
            }],
        }];
        save_history(&dir, &history);
        let loaded = load_history(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].text, "plate this");
        assert_eq!(loaded[0].chips[0].code, encode_item(24836));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
