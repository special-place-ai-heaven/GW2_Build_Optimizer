//! Bubble chat: player on the right, animated Choya on the left.

use std::path::Path;

use nexus::imgui::{ChildWindow, InputTextFlags, StyleColor, StyleVar, Ui};
use serde::{Deserialize, Serialize};

use crate::chat_links::ChatChip;
use crate::ui::{color_u32, icons, theme};
use gw2_core::i18n::t;

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
    /// Last keystroke in the composer. Bob while this is recent; otherwise sleep.
    pub last_typed: Option<std::time::Instant>,
    /// Header idle pose (0..HEADER_POSE_COUNT). Cycles about once a minute.
    pub header_pose: u8,
    pub header_pose_at: Option<std::time::Instant>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatMessage {
    pub from_user: bool,
    pub text: String,
    pub chips: Vec<ChatChip>,
    /// Clickable "Build is ready" card under this reply.
    #[serde(default)]
    pub open_result: bool,
}

pub enum ChatAction {
    Send(String),
    OpenBuild,
}

/// Maximum chat history entries retained. Beyond this, the oldest entries are
/// dropped on append so a long-running session can't grow the Vec without bound.
const CHAT_HISTORY_CAP: usize = 100;

const AVATAR: f32 = 42.0;
const AVATAR_GAP: f32 = 10.0;
const COPY: f32 = 16.0;
const COPY_GAP: f32 = 6.0;
const BUBBLE_PAD: f32 = 10.0;
const BUBBLE_ROUND: f32 = 14.0;
const COMPOSER_H: f32 = 76.0;
const COMPOSER_CHOYA: f32 = 56.0;
const SEND_SZ: f32 = 36.0;
const ROW_GAP: f32 = 12.0;

fn trim_history(history: &mut Vec<ChatMessage>) {
    if history.len() > CHAT_HISTORY_CAP {
        let drop = history.len() - CHAT_HISTORY_CAP;
        history.drain(..drop);
    }
}

/// Push a player line and return it for `send_chat_message`.
pub fn queue_user_message(state: &mut ChatBarState, msg: &str) -> Option<String> {
    let msg = msg.trim();
    if msg.is_empty() {
        return None;
    }
    state.history.push(ChatMessage {
        from_user: true,
        text: msg.to_string(),
        chips: Vec::new(),
        open_result: false,
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

fn bubble_size(ui: &Ui, text: &str, avail: f32, from_user: bool) -> (Vec<String>, f32, f32) {
    let copy_slot = if from_user { COPY + COPY_GAP } else { 0.0 };
    let max_text = ((avail - AVATAR - AVATAR_GAP - copy_slot - 24.0) * 0.78).max(72.0);
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

fn draw_copy_glyph(ui: &Ui, p: [f32; 2], size: f32, copied: bool) {
    let dl = ui.get_window_draw_list();
    let col = if copied { theme::GOLD } else { theme::MUTED };
    let back = [p[0] + size * 0.28, p[1]];
    let back_br = [p[0] + size, p[1] + size * 0.78];
    let front = [p[0], p[1] + size * 0.22];
    let front_br = [p[0] + size * 0.72, p[1] + size];
    dl.add_rect(back, back_br, col).rounding(2.0).build();
    dl.add_rect(front, front_br, col).rounding(2.0).build();
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
) -> Option<ChatAction> {
    let mut action = None;

    if state.copied_frames > 0 {
        state.copied_frames = state.copied_frames.saturating_sub(1);
        if state.copied_frames == 0 {
            state.copied_code = None;
        }
    }

    let avail_h = ui.content_region_avail()[1];
    let scroll_h = (avail_h - COMPOSER_H - 10.0).max(80.0);
    let _child_bg = ui.push_style_color(StyleColor::ChildBg, [0.05, 0.04, 0.03, 0.35]);
    ChildWindow::new("##talk_scroll")
        .size([0.0, scroll_h])
        .build(ui, || {
            let avail = ui.content_region_avail()[0];
            if state.history.is_empty() && !state.waiting {
                theme::wrapped(ui, theme::MUTED, &t("chat.placeholder_new"));
                return;
            }
            let n = state.history.len();
            for i in 0..n {
                let from_user = state.history[i].from_user;
                let text = state.history[i].text.clone();
                let open_result = state.history[i].open_result;
                let (lines, bw, bh) = bubble_size(ui, &text, avail, from_user);
                let origin = ui.cursor_screen_pos();
                let bubble_h = bh.max(AVATAR);
                ui.dummy([avail, bubble_h]);

                let (av_x, bub_x, copy_x) = if from_user {
                    let av_x = origin[0] + avail - AVATAR;
                    let copy_x = av_x - COPY_GAP - COPY;
                    (av_x, copy_x - AVATAR_GAP - bw, Some(copy_x))
                } else {
                    (origin[0], origin[0] + AVATAR + AVATAR_GAP, None)
                };
                let av_y = origin[1];
                let bub_y = origin[1];

                if from_user {
                    icons::paint_avatar(ui, user_icon, [av_x, av_y], AVATAR, user_letter);
                    if let Some(copy_x) = copy_x {
                        let copy_y = av_y + (AVATAR - COPY) * 0.5;
                        let key = format!("##msg{i}");
                        let mut copied = state.copied_code.as_deref() == Some(key.as_str())
                            && state.copied_frames > 0;
                        ui.set_cursor_screen_pos([copy_x, copy_y]);
                        if ui.invisible_button(format!("##copy_msg{i}"), [COPY, COPY])
                            && crate::clipboard::copy_text(&text)
                        {
                            state.copied_code = Some(key);
                            state.copied_frames = 120;
                            copied = true;
                        }
                        if ui.is_item_hovered() {
                            ui.tooltip_text(if copied {
                                t("chat.copied")
                            } else {
                                t("chat.copy_gw2")
                            });
                        }
                        draw_copy_glyph(ui, [copy_x, copy_y], COPY, copied);
                    }
                } else {
                    theme::draw_choya_avatar(
                        ui,
                        [av_x + AVATAR * 0.5, av_y + AVATAR * 0.5],
                        AVATAR,
                    );
                }
                draw_bubble_rect(ui, [bub_x, bub_y], bw, bh, from_user);
                draw_bubble_text(ui, [bub_x, bub_y], &lines);

                ui.set_cursor_screen_pos([bub_x, bub_y + bh + 4.0]);
                if !state.history[i].chips.is_empty() {
                    render_chips(ui, state, i, bw);
                }
                if open_result {
                    let cy = ui.cursor_screen_pos()[1] + 6.0;
                    ui.set_cursor_screen_pos([bub_x, cy]);
                    if render_build_card(ui, i) {
                        action = Some(ChatAction::OpenBuild);
                    }
                }
                let end_y = ui.cursor_screen_pos()[1].max(origin[1] + bubble_h) + ROW_GAP;
                ui.set_cursor_screen_pos([origin[0], end_y]);
            }
            if state.waiting {
                let line = cooking
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| t("choya.thinking"));
                let (lines, bw, bh) = bubble_size(ui, &line, avail, false);
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

    if let Some(send) = render_composer(ui, state) {
        action = Some(ChatAction::Send(send));
    }
    action
}

fn render_build_card(ui: &Ui, msg_i: usize) -> bool {
    const PAD_X: f32 = 14.0;
    const PAD_Y: f32 = 10.0;
    const GEM_H: f32 = 28.0;
    const GEM_GAP: f32 = 10.0;
    let title = t("chat.build_ready");
    let sub = t("chat.open_optimized");
    let title_sz = ui.calc_text_size(&title);
    let sub_sz = ui.calc_text_size(&sub);
    let text_w = title_sz[0].max(sub_sz[0]);
    let gem_w = GEM_H * (308.0 / 256.0);
    let w = PAD_X + gem_w + GEM_GAP + text_w + PAD_X;
    let text_h = title_sz[1] + 4.0 + sub_sz[1];
    let h = (text_h + PAD_Y * 2.0).max(GEM_H + PAD_Y * 2.0);
    let p = ui.cursor_screen_pos();
    let id = format!("##build_card{msg_i}");
    let clicked = ui.invisible_button(&id, [w, h]);
    let hovered = ui.is_item_hovered();
    let fill = if hovered {
        [0.22, 0.18, 0.08, 0.96]
    } else {
        theme::PLATE
    };
    {
        let dl = ui.get_window_draw_list();
        dl.add_rect(p, [p[0] + w, p[1] + h], fill)
            .filled(true)
            .rounding(10.0)
            .build();
        dl.add_rect(p, [p[0] + w, p[1] + h], theme::GOLD)
            .rounding(10.0)
            .build();
        let gem_cx = p[0] + PAD_X + gem_w * 0.5;
        let gem_top = p[1] + (h - GEM_H) * 0.5;
        theme::draw_gem_icon(&dl, [gem_cx, gem_top], GEM_H);
        let tx = p[0] + PAD_X + gem_w + GEM_GAP;
        let ty = p[1] + (h - text_h) * 0.5;
        dl.add_text([tx, ty], color_u32(theme::GOLD), &title);
        dl.add_text([tx, ty + title_sz[1] + 4.0], color_u32(theme::MUTED), &sub);
    }
    if hovered {
        ui.tooltip_text(t("chat.open_optimized"));
    }
    clicked
}

fn draw_send_icon(ui: &Ui, c: [f32; 2], on: bool) {
    let col = if on { theme::GOLD } else { theme::MUTED };
    let dl = ui.get_window_draw_list();
    let s = 11.0;
    dl.add_triangle(
        [c[0] - s * 0.7, c[1] - s * 0.55],
        [c[0] + s * 0.85, c[1]],
        [c[0] - s * 0.7, c[1] + s * 0.55],
        col,
    )
    .filled(true)
    .build();
}

fn render_composer(ui: &Ui, state: &mut ChatBarState) -> Option<String> {
    let avail = ui.content_region_avail()[0];
    let origin = ui.cursor_screen_pos();
    ui.dummy([avail, COMPOSER_H]);
    let after = ui.cursor_screen_pos();

    let choya_c = [
        origin[0] + COMPOSER_CHOYA * 0.5,
        origin[1] + COMPOSER_H * 0.52,
    ];
    if theme::composer_choya_bobbing(state.last_typed, std::time::Instant::now()) {
        theme::draw_choya_walk(ui, choya_c, COMPOSER_CHOYA);
    } else {
        theme::draw_choya_sleep(ui, choya_c, COMPOSER_CHOYA);
    }

    let bx = origin[0] + COMPOSER_CHOYA + 8.0;
    let by = origin[1];
    let bw = (avail - COMPOSER_CHOYA - 8.0).max(80.0);
    let bh = COMPOSER_H;
    {
        let dl = ui.get_window_draw_list();
        dl.add_rect([bx, by], [bx + bw, by + bh], theme::PLATE)
            .filled(true)
            .rounding(18.0)
            .build();
        dl.add_rect([bx, by], [bx + bw, by + bh], theme::GOLD_DIM)
            .rounding(18.0)
            .build();
    }

    let input_w = (bw - SEND_SZ - 20.0).max(40.0);
    ui.set_cursor_screen_pos([bx + 12.0, by + 8.0]);
    let _pad = ui.push_style_var(StyleVar::FramePadding([8.0, 8.0]));
    let _bg = ui.push_style_color(StyleColor::FrameBg, [0.0, 0.0, 0.0, 0.0]);
    let _bgh = ui.push_style_color(StyleColor::FrameBgHovered, [0.0, 0.0, 0.0, 0.0]);
    let _bga = ui.push_style_color(StyleColor::FrameBgActive, [0.0, 0.0, 0.0, 0.0]);
    let _brd = ui.push_style_var(StyleVar::FrameBorderSize(0.0));

    let enter_pressed = ui
        .input_text_multiline("##chat_input", &mut state.input, [input_w, bh - 16.0])
        .flags(
            InputTextFlags::CALLBACK_RESIZE
                | InputTextFlags::ENTER_RETURNS_TRUE
                | InputTextFlags::CTRL_ENTER_FOR_NEW_LINE,
        )
        .build();
    if ui.is_item_edited() {
        state.last_typed = Some(std::time::Instant::now());
    }
    drop(_brd);
    drop(_bga);
    drop(_bgh);
    drop(_bg);
    drop(_pad);

    if state.input.is_empty() {
        ui.get_window_draw_list().add_text(
            [bx + 20.0, by + 16.0],
            color_u32(theme::MUTED),
            t("chat.placeholder"),
        );
    }

    ui.set_cursor_screen_pos([bx + bw - SEND_SZ - 10.0, by + (bh - SEND_SZ) * 0.5]);
    let send_hit = ui.invisible_button("##chat_send", [SEND_SZ, SEND_SZ]);
    let send_on = !state.input.trim().is_empty();
    let send_p = ui.item_rect_min();
    draw_send_icon(
        ui,
        [send_p[0] + SEND_SZ * 0.5, send_p[1] + SEND_SZ * 0.5],
        send_on,
    );
    if ui.is_item_hovered() {
        ui.tooltip_text(t("chat.send_tip"));
    }

    ui.set_cursor_screen_pos(after);

    let can_send = !state.input.trim().is_empty();
    if (enter_pressed || send_hit) && can_send {
        return queue_user_message(state, &state.input.clone());
    }
    None
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
            t("chat.copied")
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
            ui.tooltip_text(t("chat.copy_gw2"));
        }
        row_x += pill_w + 4.0;
    }
}

/// Add an assistant reply with no serving chips (errors, timeout, talk).
pub fn add_ai_response(state: &mut ChatBarState, text: String) {
    add_plated_response(state, text, Vec::new(), false);
}

/// Add an assistant reply and optional GW2 chat-link chips.
pub fn add_plated_response(
    state: &mut ChatBarState,
    text: String,
    chips: Vec<ChatChip>,
    open_result: bool,
) {
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
        open_result,
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
    fn queue_user_message_while_waiting_still_queues() {
        let mut state = ChatBarState {
            waiting: true,
            ..Default::default()
        };
        assert_eq!(queue_user_message(&mut state, "hi").as_deref(), Some("hi"));
        assert_eq!(state.history.len(), 1);
        assert!(state.history[0].from_user);
        assert!(state.waiting);
        state.waiting = false;
        assert_eq!(
            queue_user_message(&mut state, "  yo  ").as_deref(),
            Some("yo")
        );
        assert_eq!(state.history.len(), 2);
    }

    #[test]
    fn recent_transcript_keeps_last_n() {
        let mut history = Vec::new();
        for i in 0..5 {
            history.push(ChatMessage {
                from_user: i % 2 == 0,
                text: format!("m{i}"),
                chips: Vec::new(),
                open_result: false,
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
        let mut state = ChatBarState {
            waiting: true,
            ..Default::default()
        };
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
        let mut state = ChatBarState {
            waiting: true,
            ..Default::default()
        };
        add_plated_response(
            &mut state,
            "Scholar rune.".into(),
            vec![ChatChip {
                kind: LinkKind::Item,
                label: "Rune of the Scholar".into(),
                code: encode_item(24836),
            }],
            true,
        );
        assert_eq!(state.history[0].chips.len(), 1);
        assert_eq!(state.history[0].chips[0].code, "[&AgEEYQAA]");
        assert!(state.history[0].open_result);
    }

    #[test]
    fn attach_order_chips_updates_last_customer_line() {
        let mut state = ChatBarState::default();
        state.history.push(ChatMessage {
            from_user: true,
            text: "[&AgEEYQAA]".into(),
            chips: Vec::new(),
            open_result: false,
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
            open_result: false,
        }];
        save_history(&dir, &history);
        let loaded = load_history(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].text, "plate this");
        assert_eq!(loaded[0].chips[0].code, encode_item(24836));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_kitchen_json_defaults_open_result_off() {
        let hist: Vec<ChatMessage> =
            serde_json::from_str(r#"[{"from_user":true,"text":"hi","chips":[]}]"#).unwrap();
        assert!(!hist[0].open_result);
        assert_eq!(hist[0].text, "hi");
    }
}
