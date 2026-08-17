//! Bubble chat: player on the right, animated Choya on the left.

use std::path::Path;

use nexus::imgui::{ChildWindow, StyleColor, Ui};
use serde::{Deserialize, Serialize};

use crate::chat_links::ChatChip;
use crate::ui::{color_u32, icons, theme};

/// State for the talk-tab transcript.
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
/// dropped on append so a long-running session can't grow the Vec without bound.
const CHAT_HISTORY_CAP: usize = 100;

const AVATAR: f32 = 42.0;
const AVATAR_GAP: f32 = 10.0;
const BUBBLE_PAD: f32 = 10.0;
const BUBBLE_ROUND: f32 = 14.0;
const COMPOSER_H: f32 = 32.0;
const ROW_GAP: f32 = 12.0;

fn trim_history(history: &mut Vec<ChatMessage>) {
    if history.len() > CHAT_HISTORY_CAP {
        let drop = history.len() - CHAT_HISTORY_CAP;
        history.drain(..drop);
    }
}

/// Push a player line and return it for `send_chat_message`. None if waiting.
pub fn queue_user_message(state: &mut ChatBarState, msg: &str) -> Option<String> {
    let msg = msg.trim();
    if state.waiting || msg.is_empty() {
        return None;
    }
    state.history.push(ChatMessage {
        from_user: true,
        text: msg.to_string(),
        chips: Vec::new(),
    });
    trim_history(&mut state.history);
    state.input.clear();
    state.scroll_to_end = true;
    state.dirty = true;
    Some(msg.to_string())
}

/// Last `n` turns for the LLM brief. Oldest first.
pub fn recent_transcript(history: &[ChatMessage], n: usize) -> String {
    let start = history.len().saturating_sub(n);
    history[start..]
        .iter()
        .map(|m| {
            let who = if m.from_user { "Player" } else { "Assistant" };
            let text: String = m.text.chars().take(240).collect();
            format!("{who}: {text}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn wrap_text(ui: &Ui, text: &str, max_w: f32) -> (Vec<String>, f32, f32) {
    let mut lines = Vec::new();
    let mut max_line_w = 0.0f32;
    let line_h = ui.calc_text_size("Ag")[1];
    for para in text.split('\n') {
        if para.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let trial = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{cur} {word}")
            };
            if !cur.is_empty() && ui.calc_text_size(&trial)[0] > max_w {
                max_line_w = max_line_w.max(ui.calc_text_size(&cur)[0]);
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            } else {
                cur = trial;
            }
        }
        if !cur.is_empty() {
            max_line_w = max_line_w.max(ui.calc_text_size(&cur)[0]);
            lines.push(cur);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    let h = (lines.len() as f32) * line_h;
    (lines, max_line_w.min(max_w), h.max(line_h))
}

fn chip_block_h(ui: &Ui, chips: &[ChatChip], max_w: f32) -> f32 {
    if chips.is_empty() {
        return 0.0;
    }
    let mut row_x = 0.0;
    let mut rows = 1u32;
    for chip in chips {
        let pill_w = ui.calc_text_size(&chip.label)[0] + 20.0;
        if row_x > 0.0 && row_x + pill_w + 4.0 > max_w {
            rows += 1;
            row_x = 0.0;
        }
        row_x += pill_w + 4.0;
    }
    rows as f32 * 22.0 + 4.0
}

fn bubble_size(ui: &Ui, text: &str, avail: f32) -> (Vec<String>, f32, f32) {
    let max_text = ((avail - AVATAR - AVATAR_GAP - 24.0) * 0.78).max(72.0);
    let (lines, text_w, text_h) = wrap_text(ui, text, max_text);
    let bw = (text_w + BUBBLE_PAD * 2.0).clamp(48.0, max_text + BUBBLE_PAD * 2.0);
    let bh = (text_h + BUBBLE_PAD * 2.0).max(AVATAR * 0.65);
    (lines, bw, bh)
}

fn draw_bubble_rect(ui: &Ui, p: [f32; 2], bw: f32, bh: f32, from_user: bool) {
    let dl = ui.get_window_draw_list();
    let fill = if from_user {
        [0.16, 0.20, 0.28, 0.96]
    } else {
        theme::PLATE
    };
    let rim = if from_user {
        [
            theme::CURRENT[0],
            theme::CURRENT[1],
            theme::CURRENT[2],
            0.55,
        ]
    } else {
        theme::GOLD_DIM
    };
    dl.add_rect(p, [p[0] + bw, p[1] + bh], fill)
        .filled(true)
        .rounding(BUBBLE_ROUND)
        .build();
    dl.add_rect(p, [p[0] + bw, p[1] + bh], rim)
        .rounding(BUBBLE_ROUND)
        .build();
}

fn draw_bubble_text(ui: &Ui, p: [f32; 2], lines: &[String]) {
    let dl = ui.get_window_draw_list();
    let line_h = ui.calc_text_size("Ag")[1];
    let mut ty = p[1] + BUBBLE_PAD;
    for line in lines {
        dl.add_text([p[0] + BUBBLE_PAD, ty], color_u32(theme::CREAM), line);
        ty += line_h;
    }
}

/// Transcript fills leftover height; composer stays pinned. `user_icon` is the
/// profession portrait when a character is selected.
pub fn render_chat_bar(
    ui: &Ui,
    state: &mut ChatBarState,
    cooking: Option<&str>,
    user_icon: Option<&str>,
    user_letter: char,
) -> Option<String> {
    let mut submitted = None;

    if state.copied_frames > 0 {
        state.copied_frames = state.copied_frames.saturating_sub(1);
        if state.copied_frames == 0 {
            state.copied_code = None;
        }
    }

    let avail_h = ui.content_region_avail()[1];
    let scroll_h = (avail_h - COMPOSER_H - 8.0).max(80.0);
    let _child_bg = ui.push_style_color(StyleColor::ChildBg, [0.05, 0.04, 0.03, 0.35]);
    ChildWindow::new("##talk_scroll")
        .size([0.0, scroll_h])
        .build(ui, || {
            let avail = ui.content_region_avail()[0];
            if state.history.is_empty() && !state.waiting {
                theme::wrapped(
                    ui,
                    theme::MUTED,
                    "Ask Choya about a new build or how to improve the selected character. Paste a GW2 chat link if you have one.",
                );
                return;
            }
            let n = state.history.len();
            for i in 0..n {
                let from_user = state.history[i].from_user;
                let text = state.history[i].text.clone();
                let (lines, bw, bh) = bubble_size(ui, &text, avail);
                let chip_h = chip_block_h(ui, &state.history[i].chips, bw);
                let row_h = bh.max(AVATAR) + chip_h + ROW_GAP;
                let origin = ui.cursor_screen_pos();
                let id = format!("##talk_row{i}");
                ui.invisible_button(&id, [avail, row_h]);
                let after = ui.cursor_screen_pos();

                let (av_x, bub_x) = if from_user {
                    let av_x = origin[0] + avail - AVATAR;
                    (av_x, av_x - AVATAR_GAP - bw)
                } else {
                    (origin[0], origin[0] + AVATAR + AVATAR_GAP)
                };
                let av_y = origin[1];
                let bub_y = origin[1];

                if from_user {
                    icons::paint_avatar(ui, user_icon, [av_x, av_y], AVATAR, user_letter);
                } else {
                    theme::draw_choya_avatar(
                        ui,
                        [av_x + AVATAR * 0.5, av_y + AVATAR * 0.5],
                        AVATAR,
                    );
                }
                draw_bubble_rect(ui, [bub_x, bub_y], bw, bh, from_user);
                draw_bubble_text(ui, [bub_x, bub_y], &lines);

                if chip_h > 0.0 {
                    ui.set_cursor_screen_pos([bub_x, bub_y + bh + 2.0]);
                    render_chips(ui, state, i, bw);
                }
                ui.set_cursor_screen_pos(after);
            }
            if state.waiting {
                let n = (ui.frame_count() / 18) % 4;
                let dots = ".".repeat(n as usize);
                let line = cooking
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("Choya is thinking{dots}"));
                let (lines, bw, bh) = bubble_size(ui, &line, avail);
                let row_h = bh.max(AVATAR) + ROW_GAP;
                let origin = ui.cursor_screen_pos();
                ui.invisible_button("##talk_thinking", [avail, row_h]);
                let av_x = origin[0];
                theme::draw_choya_thinking(
                    ui,
                    [av_x + AVATAR * 0.5, origin[1] + AVATAR * 0.5],
                    AVATAR,
                );
                let bub_x = origin[0] + AVATAR + AVATAR_GAP;
                draw_bubble_rect(ui, [bub_x, origin[1]], bw, bh, false);
                draw_bubble_text(ui, [bub_x, origin[1]], &lines);
            }
            if state.scroll_to_end {
                ui.set_scroll_here_y();
                state.scroll_to_end = false;
            }
        });
    drop(_child_bg);

    let avail_width = ui.content_region_avail()[0];
    let button_width = 64.0;
    let choya_slot = 26.0;
    let p = ui.cursor_screen_pos();
    ui.dummy([choya_slot, COMPOSER_H]);
    theme::draw_choya_walk(
        ui,
        [p[0] + choya_slot * 0.5, p[1] + COMPOSER_H * 0.45],
        24.0,
    );
    ui.same_line_with_spacing(0.0, 6.0);
    ui.set_next_item_width((avail_width - button_width - choya_slot - 16.0).max(40.0));
    if state.waiting {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        let mut dummy = String::new();
        ui.input_text("##chat_input", &mut dummy)
            .read_only(true)
            .build();
        style.pop();
        ui.same_line();
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        theme::gold_button_sized(ui, "Send", [button_width, 0.0]);
        style.pop();
        return submitted;
    }

    let enter_pressed = ui
        .input_text("##chat_input", &mut state.input)
        .hint("Ask Choya about the build\u{2026}")
        .enter_returns_true(true)
        .build();

    ui.same_line();

    let can_send = !state.input.is_empty();
    let send_clicked = theme::gold_button_sized(ui, "Send", [button_width, 0.0]) && can_send;

    if (enter_pressed || send_clicked) && can_send {
        submitted = queue_user_message(state, &state.input.clone());
    }

    submitted
}

fn render_chips(ui: &Ui, state: &mut ChatBarState, msg_i: usize, max_w: f32) {
    let n = state.history[msg_i].chips.len();
    if n == 0 {
        return;
    }
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
            if row_x + pill_w + 4.0 > max_w {
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

/// Add an assistant reply with no serving chips (errors, timeout, talk).
pub fn add_ai_response(state: &mut ChatBarState, text: String) {
    add_plated_response(state, text, Vec::new());
}

/// Add an assistant reply and optional GW2 chat-link chips.
pub fn add_plated_response(state: &mut ChatBarState, text: String, chips: Vec<ChatChip>) {
    state.waiting = false;
    // Cap for the bubble, not the suggestion panel. Char-safe (no UTF-8 panic).
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

/// Attach inbound chips to the latest player message.
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
    fn queue_user_message_skips_when_waiting() {
        let mut state = ChatBarState::default();
        state.waiting = true;
        assert!(queue_user_message(&mut state, "hi").is_none());
        assert!(state.history.is_empty());
        state.waiting = false;
        assert_eq!(
            queue_user_message(&mut state, "  hi  ").as_deref(),
            Some("hi")
        );
        assert_eq!(state.history.len(), 1);
        assert!(state.history[0].from_user);
    }

    #[test]
    fn recent_transcript_keeps_last_n() {
        let mut history = Vec::new();
        for i in 0..5 {
            history.push(ChatMessage {
                from_user: i % 2 == 0,
                text: format!("m{i}"),
                chips: Vec::new(),
            });
        }
        let t = recent_transcript(&history, 3);
        assert!(t.contains("m2"));
        assert!(t.contains("m4"));
        assert!(!t.contains("m0"));
        assert!(t.starts_with("Player: m2") || t.contains("Player: m4"));
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
            "Scholar rune.".into(),
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
