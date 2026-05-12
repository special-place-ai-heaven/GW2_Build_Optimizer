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
/// bound. Render only shows the last ~6 entries anyway.
const CHAT_HISTORY_CAP: usize = 100;

fn trim_history(history: &mut Vec<ChatMessage>) {
    if history.len() > CHAT_HISTORY_CAP {
        let drop = history.len() - CHAT_HISTORY_CAP;
        history.drain(..drop);
    }
}

/// Render the chat bar at the bottom of the build view.
/// Returns Some(message) if the user submitted a request.
pub fn render_chat_bar(ui: &Ui, state: &mut ChatBarState) -> Option<String> {
    let mut submitted = None;

    ui.separator();

    // Show recent history (last 6 messages for better context)
    let recent = state.history.len().saturating_sub(6);
    for msg in state.history[recent..].iter() {
        let (prefix, color) = if msg.from_user {
            ("> ", [0.6, 0.8, 1.0, 1.0])
        } else {
            ("AI: ", [0.3, 1.0, 0.3, 1.0])
        };
        ui.text_colored(color, &format!("{}{}", prefix, msg.text));
    }

    if state.waiting {
        ui.text_colored([1.0, 1.0, 0.0, 1.0], "Thinking...");
    }

    // Input bar
    let avail_width = ui.content_region_avail()[0];
    let button_width = 60.0;

    ui.set_next_item_width(avail_width - button_width - 10.0);
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
        let send_clicked = ui.button_with_size("Send", [button_width, 0.0]) && can_send;

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
    ui.button_with_size("Send", [button_width, 0.0]);
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
