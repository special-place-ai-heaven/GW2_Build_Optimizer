//! Bottom chat bar for conversational LLM refinement.
//! User types requests → Gemini modifies the build → changes shown above.

use nexus::imgui::Ui;

/// State for the chat bar.
#[derive(Default)]
pub struct ChatBarState {
    pub input: String,
    pub history: Vec<ChatMessage>,
    pub waiting: bool,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub from_user: bool,
    pub text: String,
}

/// Maximum chat history entries retained. Beyond this, the oldest entries are
/// dropped on append so a long-running session can't grow the Vec without
/// bound. Render only shows the last VISIBLE_HISTORY entries.
const CHAT_HISTORY_CAP: usize = 100;

fn trim_history(history: &mut Vec<ChatMessage>) {
    if history.len() > CHAT_HISTORY_CAP {
        let drop = history.len() - CHAT_HISTORY_CAP;
        history.drain(..drop);
    }
}

/// How many recent chat lines stay visible. Six lines used to eat the result
/// panel and clip Save/Copy on an 800×600 overlay.
const VISIBLE_HISTORY: usize = 2;

/// Vertical space this bar will consume (separator + history + optional
/// "Thinking..." + input row). Callers reserve this before the scroll child
/// so the input is not drawn on top of results.
pub fn reserved_height(state: &ChatBarState) -> f32 {
    let shown = state.history.len().min(VISIBLE_HISTORY) as f32;
    let waiting = if state.waiting { 18.0 } else { 0.0 };
    10.0 + shown * 18.0 + waiting + 28.0
}

/// Render the chat bar at the bottom of the build view.
/// Returns Some(message) if the user submitted a request.
pub fn render_chat_bar(ui: &Ui, state: &mut ChatBarState) -> Option<String> {
    let mut submitted = None;

    ui.separator();

    let recent = state.history.len().saturating_sub(VISIBLE_HISTORY);
    for msg in state.history[recent..].iter() {
        let (prefix, color) = if msg.from_user {
            ("You  ", crate::ui::theme::CURRENT)
        } else {
            ("Hint  ", crate::ui::theme::GOLD)
        };
        ui.text_colored(color, format!("{}{}", prefix, msg.text));
    }

    if state.waiting {
        ui.text_colored(crate::ui::theme::GOLD, "Thinking\u{2026}");
    }

    // Input bar — never let the width go negative on a narrow overlay.
    let avail_width = ui.content_region_avail()[0];
    let button_width = 60.0;
    ui.set_next_item_width((avail_width - button_width - 10.0).max(40.0));
    if state.waiting {
        // Show disabled input while waiting
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        let mut dummy = String::new();
        ui.input_text("##chat_input", &mut dummy)
            .read_only(true)
            .build();
        style.pop();
    } else {
        let enter_pressed = ui
            .input_text("##chat_input", &mut state.input)
            .enter_returns_true(true)
            .build();

        ui.same_line();

        let can_send = !state.input.is_empty();
        let send_clicked =
            crate::ui::theme::gold_button_sized(ui, "Send", [button_width, 0.0]) && can_send;

        if (enter_pressed || send_clicked) && can_send {
            let msg = state.input.trim().to_string();
            if !msg.is_empty() {
                state.history.push(ChatMessage {
                    from_user: true,
                    text: msg.clone(),
                });
                trim_history(&mut state.history);
                state.input.clear();
                submitted = Some(msg);
            }
        }

        return submitted;
    }

    ui.same_line();

    // Disabled Send button while waiting
    let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
    crate::ui::theme::gold_button_sized(ui, "Send", [button_width, 0.0]);
    style.pop();

    submitted
}

/// Add an AI response to chat history.
pub fn add_ai_response(state: &mut ChatBarState, text: String) {
    state.waiting = false;
    // Truncate long responses for chat display (char-safe to avoid UTF-8 panic)
    let display = if text.chars().count() > 200 {
        let truncated: String = text.chars().take(200).collect();
        format!("{}...", truncated)
    } else {
        text
    };
    state.history.push(ChatMessage {
        from_user: false,
        text: display,
    });
    trim_history(&mut state.history);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_height_caps_at_two_visible_lines() {
        let mut state = ChatBarState::default();
        let empty = reserved_height(&state);
        state.history.push(ChatMessage {
            from_user: true,
            text: "a".into(),
        });
        let one = reserved_height(&state);
        state.history.push(ChatMessage {
            from_user: false,
            text: "b".into(),
        });
        let two = reserved_height(&state);
        state.history.push(ChatMessage {
            from_user: true,
            text: "c".into(),
        });
        let three = reserved_height(&state);
        assert!(one > empty);
        assert!(two > one);
        assert_eq!(three, two, "only two history lines are reserved");
    }

    #[test]
    fn reserved_height_includes_waiting_line() {
        let mut state = ChatBarState::default();
        let idle = reserved_height(&state);
        state.waiting = true;
        assert!(reserved_height(&state) > idle);
    }
}
