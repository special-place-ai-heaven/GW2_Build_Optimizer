//! Radio tab — search radio-browser.info, play icecast streams in the
//! background, favorites, now-playing, choya DJ in the corner.
//!
//! Layout, top to bottom: search row, genre chips, FAVORITES (only when any
//! exist), STATIONS results in a child that takes the remaining height, and a
//! fixed-height player bar pinned at the bottom. The bar's height never
//! changes with state (error hints replace the now-playing line) so nothing
//! below the list ever shifts.

use nexus::imgui::{ChildWindow, DrawListMut, Slider, StyleColor, Ui};

use crate::radio::{art, directory, player, RadioStatus, RbStation};
use crate::state::{with_state, AddonState};
use crate::ui::theme;
use gw2_core::config::SavedStation;
use gw2_core::i18n::{t, tf};

/// Rows per directory search. The API orders by votes; 50 is plenty to scan.
const SEARCH_LIMIT: usize = 50;
const ROW_H: f32 = 40.0;
const AVATAR: f32 = 28.0;
const FAV_ROW_H: f32 = 26.0;
const FAV_AVATAR: f32 = 18.0;
const HEART: f32 = 18.0;
/// Favorites list shows this many rows before it scrolls.
const FAV_VISIBLE_ROWS: usize = 4;

/// Genre chips: i18n key suffix → radio-browser tag. The empty tag is "Top
/// stations" — no tag filter, so the vote ordering alone decides.
const GENRES: [(&str, &str); 16] = [
    ("top", ""),
    ("pop", "pop"),
    ("rock", "rock"),
    ("news", "news"),
    ("classical", "classical"),
    ("dance", "dance"),
    ("jazz", "jazz"),
    ("eighties", "80s"),
    ("oldies", "oldies"),
    ("talk", "talk"),
    ("electronic", "electronic"),
    ("country", "country"),
    ("hiphop", "hip hop"),
    ("metal", "metal"),
    ("chill", "chillout"),
    ("world", "world music"),
];

pub(in crate::ui::main_view) fn render_radio_tab(ui: &Ui, state: &mut AddonState) {
    search_row(ui, state);
    ui.dummy([0.0, 4.0]);
    genre_chips(ui, state);
    ui.dummy([0.0, 6.0]);
    favorites(ui, state);
    stations(ui, state);
    player_bar(ui, state);
}

// ---------------------------------------------------------------------------
// Search + genres
// ---------------------------------------------------------------------------

fn search_row(ui: &Ui, state: &mut AddonState) {
    let btn = t("radio.search");
    let btn_w = theme::gold_button_width(ui, &btn);
    let avail = ui.content_region_avail()[0];
    ui.set_next_item_width((avail - btn_w - 8.0).max(120.0));
    let entered = ui
        .input_text("##radio_q", &mut state.radio.search_text)
        .hint(&t("radio.search_hint"))
        .enter_returns_true(true)
        .build();
    ui.same_line_with_spacing(0.0, 8.0);
    let clicked = theme::gold_button_sized(ui, &btn, [0.0, 0.0]);
    if state.radio.searching {
        ui.same_line_with_spacing(0.0, 10.0);
        ui.align_text_to_frame_padding();
        ui.text_colored(theme::MUTED, t("radio.searching"));
    }
    if entered || clicked {
        let query = state.radio.search_text.trim().to_string();
        if !query.is_empty() {
            state.radio.selected_genre = None;
            kick_search(state, SearchKind::Name(query));
        }
    }
}

fn genre_chips(ui: &Ui, state: &mut AddonState) {
    ui.set_window_font_scale(0.85);
    ui.text_colored(theme::MUTED, t("radio.genres"));
    ui.set_window_font_scale(1.0);
    let avail = ui.content_region_avail()[0];
    let mut row_x = 0.0_f32;
    let mut pick: Option<&'static str> = None;
    for (i, (key, tag)) in GENRES.iter().enumerate() {
        let label = t(&format!("radio.genre.{key}"));
        let [w, _] = theme::select_chip_size(ui, &label, false);
        theme::wrap_chip(ui, avail, &mut row_x, w, 4.0);
        let on = state.radio.selected_genre == Some(*tag);
        if theme::select_chip(ui, &label, on, &format!("##radio_genre_{i}"), None)
            && !state.radio.searching
        {
            pick = Some(*tag);
        }
    }
    if let Some(tag) = pick {
        state.radio.selected_genre = Some(tag);
        kick_search(state, SearchKind::Tag(tag));
    }
}

enum SearchKind {
    Name(String),
    Tag(&'static str),
}

/// Start a directory-search worker, publishing back via `with_state` — same
/// shape as `news::kick`. Double-kicks are guarded by `radio.searching`.
fn kick_search(state: &mut AddonState, kind: SearchKind) {
    if state.radio.searching {
        return;
    }
    state.radio.searching = true;
    state.radio.last_error = None;
    let spawned = state.spawn_worker("radio-search", move |token| {
        // catch_unwind so a panicking search can never strand the spinner.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match &kind {
            SearchKind::Name(q) => directory::search_by_name(q, SEARCH_LIMIT),
            SearchKind::Tag(tag) => directory::search_by_tag(tag, SEARCH_LIMIT),
        }))
        .unwrap_or_else(|_| Err("station search failed".into()));
        if token.is_cancelled() {
            return;
        }
        let _ = with_state(|s| {
            s.radio.searching = false;
            match result {
                Ok(list) => s.radio.results = list,
                Err(err) => s.radio.last_error = Some(err),
            }
        });
    });
    if !spawned {
        state.radio.searching = false;
    }
}

// ---------------------------------------------------------------------------
// Favorites + station list
// ---------------------------------------------------------------------------

fn favorites(ui: &Ui, state: &mut AddonState) {
    if state.config.radio.favorites.is_empty() {
        return;
    }
    theme::header(ui, &t("radio.favorites"));
    let n = state.config.radio.favorites.len();
    let h = (n.min(FAV_VISIBLE_ROWS) as f32) * (FAV_ROW_H + 2.0) + 4.0;
    let mut play: Option<RbStation> = None;
    let mut remove: Option<String> = None;
    let current_key = current_station_key(state);
    {
        let st: &AddonState = state;
        ChildWindow::new("##radio_favs").size([0.0, h]).build(ui, || {
            for (i, f) in st.config.radio.favorites.iter().enumerate() {
                let key = fav_key(&f.stationuuid, &f.url);
                let active = current_key.as_deref() == Some(key.as_str());
                match favorite_row(ui, i, f, active) {
                    RowAction::Play => play = Some(station_from_saved(f)),
                    RowAction::Heart => remove = Some(key.clone()),
                    RowAction::None => {}
                }
            }
        });
    }
    if let Some(key) = remove {
        state
            .config
            .radio
            .favorites
            .retain(|f| fav_key(&f.stationuuid, &f.url) != key);
        crate::ui::save_config_detached(state);
    }
    if let Some(station) = play {
        start_play(state, station);
    }
}

fn stations(ui: &Ui, state: &mut AddonState) {
    theme::header(ui, &t("radio.results"));
    let bar_h = player_bar_height(ui);
    let h = (ui.content_region_avail()[1] - bar_h - 10.0).max(72.0);
    let mut play: Option<RbStation> = None;
    let mut fav_toggle: Option<RbStation> = None;
    let current_key = current_station_key(state);
    {
        let st: &AddonState = state;
        ChildWindow::new("##radio_stations")
            .size([0.0, h])
            .build(ui, || {
                if st.radio.searching {
                    ui.dummy([0.0, 8.0]);
                    theme::wrapped(ui, theme::MUTED, &t("radio.searching"));
                } else if let Some(err) = &st.radio.last_error {
                    ui.dummy([0.0, 8.0]);
                    theme::wrapped(ui, theme::ERR, err);
                    ui.set_window_font_scale(0.85);
                    theme::wrapped(ui, theme::MUTED, &t("radio.error.av_hint"));
                    ui.set_window_font_scale(1.0);
                } else if st.radio.results.is_empty() {
                    ui.dummy([0.0, 8.0]);
                    // "No stations found" only after an actual search came back
                    // empty; the untouched tab just nudges toward the heart.
                    let searched = !st.radio.search_text.trim().is_empty()
                        || st.radio.selected_genre.is_some();
                    if searched {
                        theme::wrapped(ui, theme::MUTED, &t("radio.no_results"));
                    }
                    if st.config.radio.favorites.is_empty() {
                        theme::wrapped(ui, theme::MUTED, &t("radio.no_favorites"));
                    }
                } else {
                    for (i, s) in st.radio.results.iter().enumerate() {
                        let key = fav_key(&s.stationuuid, s.stream_url());
                        let active = current_key.as_deref() == Some(key.as_str());
                        let fav = is_favorite(st, s);
                        match station_row(ui, i, s, fav, active) {
                            RowAction::Play => play = Some(s.clone()),
                            RowAction::Heart => fav_toggle = Some(s.clone()),
                            RowAction::None => {}
                        }
                    }
                }
                art::draw_corner_choya(ui, st);
            });
    }
    if let Some(station) = fav_toggle {
        toggle_favorite(state, &station, ui.frame_count() as u32);
    }
    if let Some(station) = play {
        start_play(state, station);
    }
}

enum RowAction {
    None,
    Play,
    Heart,
}

/// Two-line result row: letter-plate avatar, name, muted country • codec •
/// bitrate, heart on the right. Row click plays; the heart has its own
/// non-overlapping hit zone so a heart click never also tunes in.
fn station_row(ui: &Ui, i: usize, s: &RbStation, fav: bool, active: bool) -> RowAction {
    let origin = ui.cursor_screen_pos();
    let avail = ui.content_region_avail()[0];
    let heart_x = origin[0] + avail - HEART - 10.0;
    let row_w = (heart_x - 8.0 - origin[0]).max(40.0);
    let mut action = RowAction::None;
    if ui.invisible_button(format!("##radio_row_{i}"), [row_w, ROW_H]) {
        action = RowAction::Play;
    }
    let hovered = ui.is_item_hovered();
    let dl = ui.get_window_draw_list();
    row_plate(&dl, origin, avail, ROW_H, hovered, active);

    let letter = s.name.chars().next().unwrap_or('#');
    let av_y = origin[1] + (ROW_H - AVATAR) * 0.5;
    letter_avatar(ui, &dl, [origin[0] + 2.0, av_y], AVATAR, letter);

    let tx = origin[0] + 2.0 + AVATAR + 8.0;
    let text_w = row_w - (AVATAR + 12.0);
    let name_color = if active { theme::GOLD } else { theme::CREAM };
    let name = clip_text(ui, &s.name, text_w);
    dl.add_text([tx, origin[1] + 4.0], crate::ui::color_u32(name_color), &name);

    ui.set_window_font_scale(0.85);
    let meta = meta_line(&s.countrycode, &s.codec, s.bitrate);
    let meta = clip_text(ui, &meta, text_w);
    dl.add_text(
        [tx, origin[1] + ROW_H * 0.5 + 2.0],
        crate::ui::color_u32(theme::MUTED),
        &meta,
    );
    ui.set_window_font_scale(1.0);

    if heart_button(ui, &dl, &format!("##radio_fav_{i}"), heart_x, origin[1], ROW_H, fav) {
        action = RowAction::Heart;
    }
    ui.set_cursor_screen_pos([origin[0], origin[1] + ROW_H + 2.0]);
    action
}

/// One-line favorite row; the heart removes.
fn favorite_row(ui: &Ui, i: usize, f: &SavedStation, active: bool) -> RowAction {
    let origin = ui.cursor_screen_pos();
    let avail = ui.content_region_avail()[0];
    let heart_x = origin[0] + avail - HEART - 10.0;
    let row_w = (heart_x - 8.0 - origin[0]).max(40.0);
    let mut action = RowAction::None;
    if ui.invisible_button(format!("##radio_favrow_{i}"), [row_w, FAV_ROW_H]) {
        action = RowAction::Play;
    }
    let hovered = ui.is_item_hovered();
    let dl = ui.get_window_draw_list();
    row_plate(&dl, origin, avail, FAV_ROW_H, hovered, active);

    let letter = f.name.chars().next().unwrap_or('#');
    let av_y = origin[1] + (FAV_ROW_H - FAV_AVATAR) * 0.5;
    letter_avatar(ui, &dl, [origin[0] + 2.0, av_y], FAV_AVATAR, letter);

    let tx = origin[0] + 2.0 + FAV_AVATAR + 8.0;
    let text_w = row_w - (FAV_AVATAR + 12.0);
    let name_color = if active { theme::GOLD } else { theme::CREAM };
    let name = clip_text(ui, &f.name, text_w);
    let th = ui.calc_text_size(&name)[1];
    dl.add_text(
        [tx, origin[1] + (FAV_ROW_H - th) * 0.5],
        crate::ui::color_u32(name_color),
        &name,
    );

    if heart_button(
        ui,
        &dl,
        &format!("##radio_favdel_{i}"),
        heart_x,
        origin[1],
        FAV_ROW_H,
        true,
    ) {
        action = RowAction::Heart;
    }
    ui.set_cursor_screen_pos([origin[0], origin[1] + FAV_ROW_H + 2.0]);
    action
}

fn row_plate(dl: &DrawListMut, origin: [f32; 2], w: f32, h: f32, hovered: bool, active: bool) {
    if !hovered && !active {
        return;
    }
    let fill = if active {
        [0.22, 0.18, 0.08, 0.55]
    } else {
        theme::GOLD_HOVER
    };
    dl.add_rect(
        [origin[0] - 2.0, origin[1]],
        [origin[0] + w, origin[1] + h],
        fill,
    )
    .filled(true)
    .rounding(4.0)
    .build();
}

/// Letter-plate avatar in the `icons::paint_avatar` fallback style. Stations
/// never load remote favicons — the stream hosts trip antivirus heuristics —
/// so the letter plate IS the avatar, not a loading state.
fn letter_avatar(ui: &Ui, dl: &DrawListMut, p: [f32; 2], size: f32, letter: char) {
    let p_max = [p[0] + size, p[1] + size];
    let r = size * 0.5;
    dl.add_rect(p, p_max, theme::PLATE).filled(true).rounding(r).build();
    let s: String = letter.to_uppercase().collect();
    let sz = ui.calc_text_size(&s);
    dl.add_text(
        [p[0] + (size - sz[0]) * 0.5, p[1] + (size - sz[1]) * 0.5],
        crate::ui::color_u32(theme::CURRENT),
        &s,
    );
    dl.add_rect(p, p_max, theme::GOLD_DIM).rounding(r).build();
}

/// Heart toggle with its own hit zone; returns true on click.
fn heart_button(
    ui: &Ui,
    dl: &DrawListMut,
    id: &str,
    x: f32,
    row_y: f32,
    row_h: f32,
    fav: bool,
) -> bool {
    let y = row_y + (row_h - HEART) * 0.5;
    ui.set_cursor_screen_pos([x, y]);
    let clicked = ui.invisible_button(id, [HEART, HEART]);
    let hovered = ui.is_item_hovered();
    let color = match (fav, hovered) {
        (true, true) => [1.0, 0.45, 0.55, 1.0],
        (true, false) => [0.92, 0.30, 0.42, 1.0],
        (false, true) => [0.85, 0.45, 0.55, 0.95],
        (false, false) => [0.45, 0.42, 0.36, 0.85],
    };
    heart_glyph(dl, [x + HEART * 0.5, y + HEART * 0.5], HEART * 0.46, color);
    if hovered {
        ui.tooltip_text(t(if fav {
            "radio.remove_favorite"
        } else {
            "radio.add_favorite"
        }));
    }
    clicked
}

/// Two lobes + a point — reads as a heart down to ~12px without an icon font.
fn heart_glyph(dl: &DrawListMut, c: [f32; 2], r: f32, color: [f32; 4]) {
    let lobe = r * 0.52;
    let ly = c[1] - r * 0.30;
    dl.add_circle([c[0] - lobe * 0.92, ly], lobe, color)
        .filled(true)
        .build();
    dl.add_circle([c[0] + lobe * 0.92, ly], lobe, color)
        .filled(true)
        .build();
    dl.add_triangle(
        [c[0] - r * 0.96, ly + lobe * 0.35],
        [c[0] + r * 0.96, ly + lobe * 0.35],
        [c[0], c[1] + r * 0.95],
        color,
    )
    .filled(true)
    .build();
}

// ---------------------------------------------------------------------------
// Player bar
// ---------------------------------------------------------------------------

/// Fixed bar height: two text lines + the controls row. State never changes
/// it — the error hint borrows the now-playing line.
fn player_bar_height(ui: &Ui) -> f32 {
    ui.text_line_height() * 2.0 + theme::control_height(ui) + 20.0
}

fn player_bar(ui: &Ui, state: &mut AddonState) {
    let bar_h = player_bar_height(ui);
    let origin = ui.cursor_screen_pos();
    let w = ui.content_region_avail()[0];
    let line_h = ui.text_line_height();
    {
        let dl = ui.get_window_draw_list();
        dl.add_rect(
            [origin[0] - 2.0, origin[1]],
            [origin[0] + w + 2.0, origin[1] + bar_h],
            theme::PLATE,
        )
        .filled(true)
        .rounding(6.0)
        .build();
        dl.add_rect(
            [origin[0] - 2.0, origin[1]],
            [origin[0] + w + 2.0, origin[1] + bar_h],
            theme::GOLD_DIM,
        )
        .rounding(6.0)
        .build();

        let pad = 10.0;
        let x0 = origin[0] + pad;
        let right = origin[0] + w - pad;
        let y1 = origin[1] + 6.0;
        let y2 = y1 + line_h + 3.0;
        let status = state.radio.status.clone();
        let name = state.radio.current.as_ref().map(|c| c.name.clone());

        // Line 1: status (or LIVE badge) + station name.
        let mut x = x0;
        if status == RadioStatus::Playing {
            x += live_badge(ui, &dl, [x, y1]);
            x += 8.0;
        } else {
            let (text, color) = status_line(&status);
            let shown = clip_text(ui, &text, right - x);
            dl.add_text([x, y1], crate::ui::color_u32(color), &shown);
            x += ui.calc_text_size(&shown)[0] + 10.0;
        }
        if let Some(name) = name {
            if !matches!(status, RadioStatus::Error(_)) {
                let color = if status == RadioStatus::Playing {
                    theme::GOLD
                } else {
                    theme::CREAM
                };
                let shown = clip_text(ui, &name, right - x);
                dl.add_text([x, y1], crate::ui::color_u32(color), &shown);
            }
        }

        // Line 2: now-playing title, or the antivirus hint on error.
        ui.set_window_font_scale(0.85);
        if matches!(status, RadioStatus::Error(_)) {
            let hint = clip_text(ui, &t("radio.error.av_hint"), right - x0);
            dl.add_text([x0, y2], crate::ui::color_u32(theme::MUTED), &hint);
        } else if status == RadioStatus::Playing {
            let title = state
                .radio
                .now_playing
                .lock()
                .ok()
                .and_then(|g| g.clone());
            if let Some(title) = title {
                let label = format!("{}: ", t("radio.now_playing"));
                let lw = ui.calc_text_size(&label)[0];
                dl.add_text([x0, y2], crate::ui::color_u32(theme::MUTED), &label);
                let title = truncate_middle(&title, 72);
                let shown = clip_text(ui, &title, right - x0 - lw);
                dl.add_text([x0 + lw, y2], crate::ui::color_u32(theme::CREAM), &shown);
            }
        }
        ui.set_window_font_scale(1.0);
    }

    // Controls row: Play/Stop + volume. Widgets, so they leave the draw list.
    let pad = 10.0;
    let y3 = origin[1] + 6.0 + line_h * 2.0 + 6.0;
    ui.set_cursor_screen_pos([origin[0] + pad, y3]);
    let play_label = t("radio.play");
    let stop_label = t("radio.stop");
    let btn_w = theme::gold_button_width(ui, &play_label)
        .max(theme::gold_button_width(ui, &stop_label));
    let busy = matches!(
        state.radio.status,
        RadioStatus::Playing | RadioStatus::Connecting | RadioStatus::Stalled
    );
    if busy {
        if theme::gold_button_sized(ui, format!("{stop_label}##radio_stop"), [btn_w, 0.0]) {
            player::stop();
            state.radio.status = RadioStatus::Stopped;
        }
    } else {
        let resumable = state
            .radio
            .current
            .clone()
            .or_else(|| state.config.radio.last_station.as_ref().map(station_from_saved));
        match resumable {
            Some(station) => {
                if theme::gold_button_sized(ui, format!("{play_label}##radio_play"), [btn_w, 0.0])
                {
                    start_play(state, station);
                }
            }
            None => dim_button(ui, &format!("{play_label}##radio_play"), btn_w),
        }
    }

    ui.same_line_with_spacing(0.0, 16.0);
    ui.align_text_to_frame_padding();
    ui.text_colored(theme::MUTED, t("radio.volume"));
    ui.same_line_with_spacing(0.0, 8.0);
    let slider_w = (ui.content_region_avail()[0] - pad).clamp(90.0, 220.0);
    ui.set_next_item_width(slider_w);
    let mut volume = state.config.radio.volume_percent;
    if Slider::new("##radio_vol", 0u8, 100u8).build(ui, &mut volume) {
        state.config.radio.volume_percent = volume;
        player::set_volume(volume);
    }
    // Persist once on release, not per drag tick (same debounce idea as the
    // window-rect save in ui/mod.rs).
    if ui.is_item_deactivated_after_edit() {
        crate::ui::save_config_detached(state);
    }
    ui.set_cursor_screen_pos([origin[0], origin[1] + bar_h]);
}

/// Gold LIVE pill; returns the width consumed.
fn live_badge(ui: &Ui, dl: &DrawListMut, pos: [f32; 2]) -> f32 {
    let label = t("radio.live");
    let sz = ui.calc_text_size(&label);
    let pad_x = 7.0;
    let h = sz[1] + 3.0;
    let w = sz[0] + pad_x * 2.0;
    dl.add_rect(pos, [pos[0] + w, pos[1] + h], theme::GOLD_FILL)
        .filled(true)
        .rounding(h * 0.45)
        .build();
    dl.add_text(
        [pos[0] + pad_x, pos[1] + 1.0],
        crate::ui::color_u32([0.10, 0.08, 0.04, 1.0]),
        &label,
    );
    w
}

/// Inert muted button for "nothing to resume" — same footprint as the gold
/// Play button so the bar never shifts.
fn dim_button(ui: &Ui, label: &str, w: f32) {
    let bg = [0.15, 0.13, 0.09, 0.8];
    let _b = ui.push_style_color(StyleColor::Button, bg);
    let _h = ui.push_style_color(StyleColor::ButtonHovered, bg);
    let _a = ui.push_style_color(StyleColor::ButtonActive, bg);
    let _t = ui.push_style_color(StyleColor::Text, theme::MUTED);
    let _ = ui.button_with_size(label, [w, theme::control_height(ui)]);
}

fn status_line(status: &RadioStatus) -> (String, [f32; 4]) {
    match status {
        RadioStatus::Idle => (t("radio.status.idle"), theme::MUTED),
        RadioStatus::Connecting => (t("radio.status.connecting"), theme::GOLD),
        RadioStatus::Playing => (t("radio.live"), theme::GOLD),
        RadioStatus::Stalled => (t("radio.status.stalled"), theme::WARN),
        RadioStatus::Stopped => (t("radio.status.stopped"), theme::MUTED),
        RadioStatus::DeviceLost => (t("radio.status.device_lost"), theme::ERR),
        RadioStatus::Error(msg) => (format!("{} — {}", t("radio.status.error"), msg), theme::ERR),
    }
}

// ---------------------------------------------------------------------------
// Actions + small pure helpers
// ---------------------------------------------------------------------------

/// Tune in and reflect it immediately; the playback thread confirms (or
/// corrects) the status a moment later through `with_state`.
fn start_play(state: &mut AddonState, station: RbStation) {
    player::play(&station);
    state.radio.status = RadioStatus::Connecting;
    state.radio.current = Some(station);
}

fn toggle_favorite(state: &mut AddonState, station: &RbStation, now_frames: u32) {
    let key = fav_key(&station.stationuuid, station.stream_url());
    let favorites = &mut state.config.radio.favorites;
    if let Some(i) = favorites
        .iter()
        .position(|f| fav_key(&f.stationuuid, &f.url) == key)
    {
        favorites.remove(i);
    } else {
        favorites.push(saved_from_station(station));
        art::flash_love(now_frames);
    }
    crate::ui::save_config_detached(state);
}

fn is_favorite(state: &AddonState, station: &RbStation) -> bool {
    let key = fav_key(&station.stationuuid, station.stream_url());
    state
        .config
        .radio
        .favorites
        .iter()
        .any(|f| fav_key(&f.stationuuid, &f.url) == key)
}

fn current_station_key(state: &AddonState) -> Option<String> {
    state
        .radio
        .current
        .as_ref()
        .map(|c| fav_key(&c.stationuuid, c.stream_url()))
}

/// Favorite identity: stationuuid, falling back to the trimmed stream URL for
/// hand-entered stations without one (matches the `RadioPreferences` doc).
fn fav_key(uuid: &str, url: &str) -> String {
    if uuid.is_empty() {
        url.trim().to_string()
    } else {
        uuid.to_string()
    }
}

fn saved_from_station(s: &RbStation) -> SavedStation {
    SavedStation {
        stationuuid: s.stationuuid.clone(),
        name: s.name.clone(),
        url: s.stream_url().to_string(),
        favicon: s.favicon.clone(),
        codec: s.codec.clone(),
        bitrate: s.bitrate,
        countrycode: s.countrycode.clone(),
        tags: s.tags.clone(),
    }
}

fn station_from_saved(f: &SavedStation) -> RbStation {
    RbStation {
        stationuuid: f.stationuuid.clone(),
        name: f.name.clone(),
        url: f.url.clone(),
        url_resolved: f.url.clone(),
        favicon: f.favicon.clone(),
        tags: f.tags.clone(),
        countrycode: f.countrycode.clone(),
        codec: f.codec.clone(),
        bitrate: f.bitrate,
        lastcheckok: 1,
        hls: 0,
    }
}

/// country • codec • bitrate, skipping whatever a sparse record lacks.
fn meta_line(country: &str, codec: &str, bitrate: u32) -> String {
    let bits = if bitrate > 0 {
        tf("fmt.radio_bitrate", &[("kbps", bitrate.to_string().as_str())])
    } else {
        String::new()
    };
    join_meta([country, codec, &bits])
}

fn join_meta(parts: [&str; 3]) -> String {
    parts
        .iter()
        .filter(|p| !p.trim().is_empty())
        .map(|p| p.trim())
        .collect::<Vec<_>>()
        .join(" • ")
}

/// Width-aware end truncation with an ellipsis — UTF-8 safe (chars, never
/// byte slices), mirroring the private `theme::clip_to_width`.
fn clip_text(ui: &Ui, text: &str, max_w: f32) -> String {
    if max_w <= 4.0 {
        return String::new();
    }
    if ui.calc_text_size(text)[0] <= max_w {
        return text.to_string();
    }
    let mut s = String::new();
    for ch in text.chars() {
        let mut probe = s.clone();
        probe.push(ch);
        probe.push('…');
        if ui.calc_text_size(&probe)[0] > max_w {
            break;
        }
        s.push(ch);
    }
    s.push('…');
    s
}

/// Middle truncation for long now-playing titles (no marquee in v1): keeps
/// both the artist-ish head and the title-ish tail readable.
fn truncate_middle(text: &str, max_chars: usize) -> String {
    let n = text.chars().count();
    if n <= max_chars || max_chars < 3 {
        return text.to_string();
    }
    let keep = max_chars - 1;
    let head = keep - keep / 2;
    let tail = keep / 2;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(n - tail).collect();
    format!("{start}…{end}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genre_chips_cover_the_translated_set() {
        assert_eq!(GENRES.len(), 16);
        // "Top stations" is the tagless vote-order search.
        assert_eq!(GENRES[0], ("top", ""));
        // Multi-word radio-browser tags stay exactly as the directory spells them.
        assert!(GENRES.contains(&("hiphop", "hip hop")));
        assert!(GENRES.contains(&("world", "world music")));
        assert!(GENRES.contains(&("chill", "chillout")));
        assert!(GENRES.contains(&("eighties", "80s")));
    }

    #[test]
    fn fav_key_prefers_uuid_and_falls_back_to_trimmed_url() {
        assert_eq!(fav_key("abc", "http://x/"), "abc");
        assert_eq!(fav_key("", "  http://x/  "), "http://x/");
    }

    #[test]
    fn join_meta_skips_empty_parts() {
        assert_eq!(join_meta(["DE", "MP3", "128 kbps"]), "DE • MP3 • 128 kbps");
        assert_eq!(join_meta(["", "MP3", ""]), "MP3");
        assert_eq!(join_meta(["", "", ""]), "");
    }

    #[test]
    fn truncate_middle_is_utf8_safe_and_bounded() {
        assert_eq!(truncate_middle("short", 10), "short");
        let t = truncate_middle("Пример длинного названия трека в эфире", 12);
        assert_eq!(t.chars().count(), 12);
        assert!(t.contains('…'));
        // Head and tail both survive.
        assert!(t.starts_with("Пример"));
        assert!(t.ends_with("ире"));
    }

    #[test]
    fn saved_station_round_trips_to_a_playable_row() {
        let s = RbStation {
            stationuuid: "u1".into(),
            name: "Radio Tyria".into(),
            url: "http://raw/".into(),
            url_resolved: "http://resolved/".into(),
            codec: "MP3".into(),
            bitrate: 128,
            countrycode: "DE".into(),
            ..Default::default()
        };
        let saved = saved_from_station(&s);
        assert_eq!(saved.url, "http://resolved/");
        let back = station_from_saved(&saved);
        assert_eq!(back.stream_url(), "http://resolved/");
        assert_eq!(back.lastcheckok, 1);
        assert_eq!(back.hls, 0);
    }
}
