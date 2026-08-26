//! Talk tab — Choya chat. Not a footer on Improve / New Build.

use nexus::imgui::Ui;

use crate::state::{AddonState, MainTab};
use crate::ui::chat_bar::ChatAction;
use crate::ui::theme;
use gw2_core::i18n::t;

const MASCOT: f32 = 132.0;

const STARTERS: &[(&str, &str)] = &[
    (
        "starter.power",
        "Optimize a power DPS build for my current mode and role.",
    ),
    (
        "starter.condi",
        "Best condition DPS build for raids and strikes.",
    ),
    (
        "starter.sustain",
        "How should I trade survivability vs damage on this character?",
    ),
    ("starter.wvw", "Build me a WvW roaming loadout."),
    (
        "starter.improve",
        "Improve my current equipped build. Keep the playstyle, raise the weak axes.",
    ),
];

pub(in crate::ui::main_view) fn render_talk_tab(ui: &Ui, state: &mut AddonState) {
    render_choya_identity(ui, state);

    ui.spacing();
    theme::wrapped(ui, theme::MUTED, &talk_context(state));
    ui.spacing();
    render_starters(ui, state);
    ui.spacing();

    let user_icon = state.main.current_build.as_ref().and_then(|b| {
        state.main.game_db.as_ref().and_then(|db| {
            crate::ui::icons::profession_icon_url(db, &b.profession).map(str::to_string)
        })
    });
    let user_letter = state
        .main
        .selected_character
        .and_then(|i| state.main.characters.get(i))
        .and_then(|n| n.chars().next())
        .unwrap_or('?');

    let cooking = if state.main.chat.waiting {
        Some(state.main.optimize_stage.clone())
    } else {
        None
    };
    match crate::ui::chat_bar::render_chat_bar(
        ui,
        &mut state.main.chat,
        cooking.as_deref(),
        user_icon.as_deref(),
        user_letter,
    ) {
        Some(ChatAction::Send(msg)) => {
            crate::ui::main_view::chat_flow::send_chat_message(state, msg)
        }
        Some(ChatAction::OpenBuild) => open_optimized_tab(state),
        None => {}
    }
}

fn open_optimized_tab(state: &mut AddonState) {
    state.main.active_tab = if state.main.current_build.is_some() {
        MainTab::Improve
    } else {
        MainTab::NewBuild
    };
    state.main.tab_alert = None;
    state.main.comparison.show_optimized = true;
    if state.main.active_tab == MainTab::Improve {
        if let Some(ref build) = state.main.current_build {
            let build_clone = build.clone();
            super::super::resolution::auto_populate_locks(
                &build_clone,
                &mut state.main.build_locks,
            );
        }
    }
}

fn render_choya_identity(ui: &Ui, state: &mut AddonState) {
    // Hat, maracas, and orbiting notes draw outside the body quad.
    const PAD_L: f32 = 28.0;
    const PAD_T: f32 = 44.0;
    const PAD_R: f32 = 28.0;
    const PAD_B: f32 = 14.0;
    let box_w = PAD_L + MASCOT + PAD_R;
    let box_h = PAD_T + MASCOT + PAD_B;
    let top = ui.cursor_screen_pos();
    ui.invisible_button("##choya_mascot", [box_w, box_h]);
    let below = ui.cursor_screen_pos();
    let center = [top[0] + PAD_L + MASCOT * 0.5, top[1] + PAD_T + MASCOT * 0.5];
    theme::tick_header_pose(
        &mut state.main.chat.header_pose,
        &mut state.main.chat.header_pose_at,
        ui.frame_count() as u32,
        std::time::Instant::now(),
    );
    theme::draw_choya_header(
        ui,
        center,
        MASCOT,
        state.main.chat.waiting,
        state.main.chat.header_pose,
    );

    let text_x = top[0] + box_w + 10.0;
    let ty0 = top[1] + PAD_T + 8.0;
    ui.set_cursor_screen_pos([text_x, ty0]);
    ui.text_colored(theme::GOLD, t("tab.choya"));
    if !state.main.chat.history.is_empty() && !state.main.chat.waiting {
        ui.same_line_with_spacing(0.0, 12.0);
        if ui.small_button(format!("{}##talk", t("btn.clear"))) {
            state.main.chat.history.clear();
            state.main.chat.copied_code = None;
            state.main.chat.copied_frames = 0;
            state.main.chat.dirty = true;
        }
    }

    ui.set_cursor_screen_pos([text_x, ty0 + ui.text_line_height() + 4.0]);
    ui.text_colored(theme::MUTED, t("choya.assistant"));
    ui.same_line_with_spacing(0.0, 10.0);
    let online = state.config.has_active_llm_key();
    let pip = if online {
        theme::OPTIMIZED
    } else {
        theme::MUTED
    };
    let status = if online {
        t("status.online")
    } else {
        t("status.set_api_key")
    };
    let p = ui.cursor_screen_pos();
    let th = ui.calc_text_size(&status)[1];
    ui.get_window_draw_list()
        .add_circle([p[0] + 5.0, p[1] + th * 0.5], 4.0, pip)
        .filled(true)
        .build();
    ui.dummy([12.0, th]);
    ui.same_line();
    ui.text_colored(pip, &status);

    ui.set_cursor_screen_pos([text_x, ty0 + ui.text_line_height() * 2.0 + 12.0]);
    if let Some(issue) = state.main.provider_issue.clone() {
        theme::wrapped(ui, theme::ERR, &issue);
        ui.set_cursor_screen_pos([text_x, ui.cursor_screen_pos()[1] + 4.0]);
    }
    super::settings::render_talk_model_row(ui, state);
    if let Some(err) = state.main.models_error.clone() {
        ui.set_cursor_screen_pos([text_x, ui.cursor_screen_pos()[1]]);
        theme::wrapped(ui, theme::WARN, &err);
    }

    let after_id = ui.cursor_screen_pos();
    ui.set_cursor_screen_pos([top[0], below[1].max(after_id[1])]);
}

fn render_starters(ui: &Ui, state: &mut AddonState) {
    let avail = ui.content_region_avail()[0];
    let mut row_x = 0.0;
    let mut send: Option<String> = None;
    for (i, (label_key, prompt)) in STARTERS.iter().enumerate() {
        let label = t(label_key);
        let pill_w = ui.calc_text_size(&label)[0] + 20.0;
        if i > 0 {
            if row_x + pill_w + 4.0 > avail {
                row_x = 0.0;
            } else {
                ui.same_line_with_spacing(0.0, 4.0);
            }
        }
        let id = format!("##choya_ask{i}");
        if theme::pill(ui, &label, false, &id) {
            send = Some((*prompt).to_string());
        }
        row_x += pill_w + 4.0;
    }
    if let Some(prompt) = send {
        if let Some(msg) = crate::ui::chat_bar::queue_user_message(&mut state.main.chat, &prompt) {
            crate::ui::main_view::chat_flow::send_chat_message(state, msg);
        }
    }
}

fn talk_context(state: &AddonState) -> String {
    let who = state
        .main
        .selected_character
        .and_then(|i| state.main.characters.get(i))
        .cloned()
        .unwrap_or_else(|| t("talk.no_character"));
    let prof = state
        .main
        .current_build
        .as_ref()
        .map(|b| b.profession.clone())
        .unwrap_or_else(|| t("talk.any_profession"));
    let role = state
        .main
        .selected_role
        .map(super::super::role_i18n_key)
        .map(t)
        .unwrap_or_else(|| t("talk.no_role"));
    format!(
        "{} \u{00b7} {} \u{00b7} {} \u{00b7} {}",
        who,
        prof,
        state.main.game_mode.label(),
        role
    )
}
