//! Radio tab — search radio-browser.info, play icecast streams in the
//! background, favorites, now-playing, choya DJ in the corner.
//!
//! Layout, top to bottom: search row, genre chips, FAVORITES (only when any
//! exist), STATIONS results in a child that takes the remaining height, and a
//! fixed-height player bar pinned at the bottom. The bar's height never
//! changes with state (error hints replace the now-playing line) so nothing
//! below the list ever shifts.

use nexus::imgui::{ChildWindow, ComboBox, DrawListMut, Selectable, Slider, StyleColor, Ui};

use crate::radio::{art, directory, logos, player, RadioSort, RadioStatus, RbStation};
use crate::state::{with_state, AddonState};
use crate::ui::theme;
use gw2_core::config::SavedStation;
use gw2_core::i18n::{t, tf, LANGUAGES};

/// Rows per directory search. The API orders by votes; 50 is plenty to scan.
const SEARCH_LIMIT: usize = 50;
const ROW_H: f32 = 68.0;
const AVATAR: f32 = 56.0;
/// Vertical gap between station rows — the list breathes.
const ROW_GAP: f32 = 8.0;
const FAV_ROW_H: f32 = 44.0;
const FAV_AVATAR: f32 = 36.0;
const HEART: f32 = 22.0;
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
    filter_row(ui, state);
    ui.dummy([0.0, 4.0]);
    genre_chips(ui, state);
    ui.dummy([0.0, 6.0]);
    favorites(ui, state);
    stations(ui, state);
    // One pass per frame: start a logo download worker for whatever the
    // visible rows enqueued above (dedupe-guarded, single-flight).
    logos::kick(state);
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

/// UI locale code -> radio-browser language NAME (its `language=` search
/// param filters on names, not ISO codes).
const LOCALE_TO_RB: [(&str, &str); 12] = [
    ("en", "english"),
    ("de", "german"),
    ("es", "spanish"),
    ("fr", "french"),
    ("it", "italian"),
    ("nl", "dutch"),
    ("pl", "polish"),
    ("pt", "portuguese"),
    ("ru", "russian"),
    ("ja", "japanese"),
    ("ko", "korean"),
    ("zh", "chinese"),
];

/// Country combo: ISO 3166-1 alpha-2 -> English name. English throughout —
/// internationally recognizable, needs no translation into 12 locales, and
/// safe in every glyph set the overlay ships.
const COUNTRIES: [(&str, &str); 34] = [
    ("US", "United States"),
    ("GB", "United Kingdom"),
    ("IE", "Ireland"),
    ("CA", "Canada"),
    ("AU", "Australia"),
    ("DE", "Germany"),
    ("AT", "Austria"),
    ("CH", "Switzerland"),
    ("FR", "France"),
    ("BE", "Belgium"),
    ("NL", "Netherlands"),
    ("ES", "Spain"),
    ("PT", "Portugal"),
    ("IT", "Italy"),
    ("PL", "Poland"),
    ("CZ", "Czechia"),
    ("SI", "Slovenia"),
    ("HR", "Croatia"),
    ("HU", "Hungary"),
    ("RO", "Romania"),
    ("GR", "Greece"),
    ("SE", "Sweden"),
    ("NO", "Norway"),
    ("DK", "Denmark"),
    ("FI", "Finland"),
    ("RU", "Russia"),
    ("BR", "Brazil"),
    ("MX", "Mexico"),
    ("AR", "Argentina"),
    ("TR", "Turkey"),
    ("JP", "Japan"),
    ("KR", "South Korea"),
    ("CN", "China"),
    ("IN", "India"),
];

/// Language + country filter combos. Persisted in config; a change re-runs
/// whatever search context is active so the list updates immediately.
fn filter_row(ui: &Ui, state: &mut AddonState) {
    let font_pref = state.config.ui_font.clone();
    let ui_lang = state.config.ui_language.clone();
    let mut changed = false;

    ui.align_text_to_frame_padding();
    ui.set_window_font_scale(0.85);
    ui.text_colored(theme::MUTED, t("radio.language"));
    ui.set_window_font_scale(1.0);
    ui.same_line_with_spacing(0.0, 6.0);
    let cur_lang = state.config.radio.language_filter.clone();
    let preview = language_filter_label(&cur_lang, &font_pref, &ui_lang);
    ui.set_next_item_width(theme::combo_width_for(ui, &preview).max(150.0));
    if let Some(_c) = ComboBox::new("##radio_lang")
        .preview_value(&preview)
        .begin(ui)
    {
        for value in ["auto", "any"]
            .into_iter()
            .chain(LOCALE_TO_RB.iter().map(|(_, rb)| *rb))
        {
            let label = language_filter_label(value, &font_pref, &ui_lang);
            if Selectable::new(format!("{label}##radio_lang_{value}"))
                .selected(cur_lang == value)
                .build(ui)
                && cur_lang != value
            {
                state.config.radio.language_filter = value.to_string();
                changed = true;
            }
        }
    }

    ui.same_line_with_spacing(0.0, 14.0);
    ui.set_window_font_scale(0.85);
    ui.text_colored(theme::MUTED, t("radio.country"));
    ui.set_window_font_scale(1.0);
    ui.same_line_with_spacing(0.0, 6.0);
    let cur_cc = state.config.radio.country_filter.clone();
    let cc_preview = country_filter_label(&cur_cc);
    ui.set_next_item_width(theme::combo_width_for(ui, &cc_preview).max(150.0));
    if let Some(_c) = ComboBox::new("##radio_cc")
        .preview_value(&cc_preview)
        .begin(ui)
    {
        for value in ["any"]
            .into_iter()
            .chain(COUNTRIES.iter().map(|(cc, _)| *cc))
        {
            let label = country_filter_label(value);
            if Selectable::new(format!("{label}##radio_cc_{value}"))
                .selected(cur_cc == value)
                .build(ui)
                && cur_cc != value
            {
                state.config.radio.country_filter = value.to_string();
                changed = true;
            }
        }
    }

    ui.same_line_with_spacing(0.0, 14.0);
    ui.set_window_font_scale(0.85);
    ui.align_text_to_frame_padding();
    ui.text_colored(theme::MUTED, t("radio.bitrate"));
    ui.set_window_font_scale(1.0);
    ui.same_line_with_spacing(0.0, 6.0);
    let cur_cap = state.config.radio.bitrate_max;
    let cap_preview = bitrate_cap_label(ui, cur_cap);
    ui.set_next_item_width(theme::combo_width_for(ui, &cap_preview).max(110.0));
    if let Some(_c) = ComboBox::new("##radio_cap")
        .preview_value(&cap_preview)
        .begin(ui)
    {
        for cap in [0u32, 64, 128, 192] {
            let label = bitrate_cap_label(ui, cap);
            if Selectable::new(format!("{label}##radio_cap_{cap}"))
                .selected(cur_cap == cap)
                .build(ui)
                && cur_cap != cap
            {
                state.config.radio.bitrate_max = cap;
                changed = true;
            }
        }
    }

    if changed {
        crate::ui::save_config_detached(state);
        rekick_current(state);
    }
}

/// "Any" or "128 kbps" — a cap for poor connections, reusing the existing
/// bitrate format key (ASCII only; the game font lacks a <= glyph).
fn bitrate_cap_label(_ui: &Ui, cap: u32) -> String {
    if cap == 0 {
        t("radio.filter_any")
    } else {
        tf("fmt.radio_bitrate", &[("kbps", &cap.to_string())])
    }
}

fn sort_label(sort: RadioSort) -> String {
    match sort {
        RadioSort::Popular => t("radio.sort.popular"),
        RadioSort::Name => t("radio.sort.name"),
        RadioSort::Bitrate => t("radio.bitrate"),
        RadioSort::Country => t("radio.country"),
    }
}

/// Client-side result ordering; stable, so equal keys keep the API's vote
/// order as the tiebreak.
fn apply_sort(list: &mut [RbStation], sort: RadioSort) {
    match sort {
        RadioSort::Popular => list.sort_by(|a, b| b.votes.cmp(&a.votes)),
        RadioSort::Name => list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        RadioSort::Bitrate => list.sort_by(|a, b| b.bitrate.cmp(&a.bitrate)),
        RadioSort::Country => list.sort_by(|a, b| {
            a.countrycode
                .cmp(&b.countrycode)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        }),
    }
}

/// Display label for a language-filter value ("auto"/"any"/rb name). Native
/// names via the same CJK-gated helper the Settings language list uses.
fn language_filter_label(value: &str, font_pref: &str, ui_lang: &str) -> String {
    match value {
        "auto" => t("radio.filter_auto"),
        "any" => t("radio.filter_any"),
        rb => LOCALE_TO_RB
            .iter()
            .find(|(_, name)| *name == rb)
            .and_then(|(code, _)| LANGUAGES.iter().find(|l| l.code == *code))
            .map(|l| crate::ui::fonts::language_label(l, font_pref, ui_lang).to_string())
            .unwrap_or_else(|| rb.to_string()),
    }
}

fn country_filter_label(value: &str) -> String {
    if value == "any" {
        return t("radio.filter_any");
    }
    COUNTRIES
        .iter()
        .find(|(cc, _)| *cc == value)
        .map(|(_, name)| (*name).to_string())
        .unwrap_or_else(|| value.to_string())
}

/// Resolve the persisted filter settings into directory search params.
fn active_filters(state: &AddonState) -> directory::SearchFilters {
    let language = match state.config.radio.language_filter.as_str() {
        "any" => None,
        "auto" => {
            let ui = gw2_core::i18n::current();
            LOCALE_TO_RB
                .iter()
                .find(|(code, _)| *code == ui)
                .map(|(_, rb)| (*rb).to_string())
        }
        rb => Some(rb.to_string()),
    };
    let countrycode = match state.config.radio.country_filter.as_str() {
        "any" => None,
        cc => Some(cc.to_string()),
    };
    directory::SearchFilters {
        language,
        countrycode,
        max_bitrate: match state.config.radio.bitrate_max {
            0 => None,
            n => Some(n),
        },
    }
}

/// Re-run whatever search context is active (name if typed, else the
/// selected genre chip, else Top stations) — used when a filter changes.
fn rekick_current(state: &mut AddonState) {
    let query = state.radio.search_text.trim().to_string();
    if !query.is_empty() {
        kick_search(state, SearchKind::Name(query));
    } else {
        let tag = state.radio.selected_genre.unwrap_or("");
        state.radio.selected_genre = Some(tag);
        kick_search(state, SearchKind::Tag(tag));
    }
}

/// Start a directory-search worker, publishing back via `with_state` — same
/// shape as `news::kick`. Double-kicks are guarded by `radio.searching`.
fn kick_search(state: &mut AddonState, kind: SearchKind) {
    if state.radio.searching {
        return;
    }
    state.radio.searching = true;
    state.radio.last_error = None;
    let filters = active_filters(state);
    let spawned = state.spawn_worker("radio-search", move |token| {
        // catch_unwind so a panicking search can never strand the spinner.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match &kind {
            SearchKind::Name(q) => directory::search_by_name(q, &filters, SEARCH_LIMIT),
            SearchKind::Tag(tag) => directory::search_by_tag(tag, &filters, SEARCH_LIMIT),
        }))
        .unwrap_or_else(|_| Err("station search failed".into()));
        if token.is_cancelled() {
            return;
        }
        let _ = with_state(|s| {
            s.radio.searching = false;
            match result {
                Ok(mut list) => {
                    apply_sort(&mut list, s.radio.sort);
                    s.radio.results = list;
                }
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
    let h = (n.min(FAV_VISIBLE_ROWS) as f32) * (FAV_ROW_H + 4.0) + 4.0;
    let mut play: Option<RbStation> = None;
    let mut remove: Option<String> = None;
    let current_key = current_station_key(state);
    {
        let st: &AddonState = state;
        ChildWindow::new("##radio_favs")
            .size([0.0, h])
            .build(ui, || {
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
    // The header is a draw-list bar plus a dummy — same_line after it lands
    // back on top of the title. Place the sort combo explicitly, right-
    // aligned INSIDE the 22px header bar, then restore the cursor.
    let header_pos = ui.cursor_screen_pos();
    let header_w = ui.content_region_avail()[0];
    theme::header(ui, &t("radio.results"));
    let after_header = ui.cursor_screen_pos();
    let cur_sort = state.radio.sort;
    let preview = sort_label(cur_sort);
    let combo_w = theme::combo_width_for(ui, &preview).max(110.0);
    let sort_lbl = t("radio.sort");
    ui.set_window_font_scale(0.85);
    let lbl_sz = ui.calc_text_size(&sort_lbl);
    {
        let dl = ui.get_window_draw_list();
        dl.add_text(
            [
                header_pos[0] + header_w - combo_w - 10.0 - lbl_sz[0],
                header_pos[1] + ((22.0 - lbl_sz[1]) * 0.5).round(),
            ],
            crate::ui::color_u32(theme::MUTED),
            &sort_lbl,
        );
    }
    ui.set_window_font_scale(1.0);
    ui.set_cursor_screen_pos([
        header_pos[0] + header_w - combo_w - 4.0,
        header_pos[1] + 1.0,
    ]);
    ui.set_next_item_width(combo_w);
    if let Some(_c) = ComboBox::new("##radio_sort")
        .preview_value(&preview)
        .begin(ui)
    {
        for (i, opt) in [
            RadioSort::Popular,
            RadioSort::Name,
            RadioSort::Bitrate,
            RadioSort::Country,
        ]
        .into_iter()
        .enumerate()
        {
            if Selectable::new(format!("{}##radio_sort_{i}", sort_label(opt)))
                .selected(cur_sort == opt)
                .build(ui)
                && cur_sort != opt
            {
                state.radio.sort = opt;
                apply_sort(&mut state.radio.results, opt);
            }
        }
    }
    ui.set_cursor_screen_pos(after_header);
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

    let av_y = origin[1] + (ROW_H - AVATAR) * 0.5;
    station_avatar(
        ui,
        &dl,
        [origin[0] + 2.0, av_y],
        AVATAR,
        &s.name,
        &s.favicon,
    );

    let tx = origin[0] + 2.0 + AVATAR + 10.0;
    let text_w = row_w - (AVATAR + 14.0);
    let name_color = if active { theme::GOLD } else { theme::CREAM };
    let name = clip_text(ui, &s.name, text_w);
    let lh = ui.text_line_height();
    dl.add_text(
        [tx, origin[1] + ROW_H * 0.5 - lh - 2.0],
        crate::ui::color_u32(name_color),
        &name,
    );

    ui.set_window_font_scale(0.85);
    let meta = meta_line(&s.countrycode, &s.codec, s.bitrate);
    let meta = clip_text(ui, &meta, text_w);
    dl.add_text(
        [tx, origin[1] + ROW_H * 0.5 + 3.0],
        crate::ui::color_u32(theme::MUTED),
        &meta,
    );
    ui.set_window_font_scale(1.0);

    if heart_button(
        ui,
        &dl,
        &format!("##radio_fav_{i}"),
        heart_x,
        origin[1],
        ROW_H,
        fav,
    ) {
        action = RowAction::Heart;
    }
    ui.set_cursor_screen_pos([origin[0], origin[1] + ROW_H + ROW_GAP]);
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

    let av_y = origin[1] + (FAV_ROW_H - FAV_AVATAR) * 0.5;
    station_avatar(
        ui,
        &dl,
        [origin[0] + 2.0, av_y],
        FAV_AVATAR,
        &f.name,
        &f.favicon,
    );

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
    ui.set_cursor_screen_pos([origin[0], origin[1] + FAV_ROW_H + 4.0]);
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

/// Station avatar: the real logo once `radio::logos` has the favicon cached
/// and decoded, the letter plate while it loads — or permanently, when the
/// favicon is missing, failed, undecodable (.ico/.webp/.svg), or the session
/// texture budget is spent. Only rows inside the child's clip rect ask the
/// pipeline, so a search result never queues 50 downloads at once.
fn station_avatar(ui: &Ui, dl: &DrawListMut, p: [f32; 2], size: f32, name: &str, favicon: &str) {
    let p_max = [p[0] + size, p[1] + size];
    if !favicon.is_empty() && ui.is_rect_visible(p, p_max) {
        if let Some(tid) = logos::texture(favicon) {
            let r = size * 0.5;
            dl.add_image_rounded(tid, p, p_max, r)
                .col([1.0, 1.0, 1.0, 1.0])
                .build();
            dl.add_rect(p, p_max, theme::GOLD_DIM).rounding(r).build();
            return;
        }
    }
    letter_avatar(ui, dl, p, size, name.chars().next().unwrap_or('#'));
}

/// Letter-plate avatar in the `icons::paint_avatar` fallback style — the
/// immediate stand-in while a favicon downloads, and the permanent avatar for
/// stations without a usable one.
fn letter_avatar(ui: &Ui, dl: &DrawListMut, p: [f32; 2], size: f32, letter: char) {
    let p_max = [p[0] + size, p[1] + size];
    let r = size * 0.5;
    dl.add_rect(p, p_max, theme::PLATE)
        .filled(true)
        .rounding(r)
        .build();
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
        // Equalizer bars live between the plate and everything else, so all
        // text and controls render on top of them.
        eq_bars(&dl, origin, w, bar_h);

        // DJ choya at the right end of the bar, clipped to it — it can no
        // longer cover the station list's hearts. Text stays clear of the
        // width it reserves.
        let choya_w = art::draw_dj_choya(
            &dl,
            state,
            ui.frame_count() as u32,
            [origin[0] - 2.0, origin[1]],
            [origin[0] + w + 2.0, origin[1] + bar_h],
        );

        let pad = 10.0;
        let x0 = origin[0] + pad;
        let right = origin[0] + w - pad - choya_w;
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
            let title = state.radio.now_playing.lock().ok().and_then(|g| g.clone());
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

    // Controls row: Play/Pause/Stop + volume. Widgets, so they leave the
    // draw list. All swap-set buttons share one width so nothing shifts.
    let pad = 10.0;
    let y3 = origin[1] + 6.0 + line_h * 2.0 + 6.0;
    ui.set_cursor_screen_pos([origin[0] + pad, y3]);
    let play_label = t("radio.play");
    let stop_label = t("radio.stop");
    let pause_label = t("radio.pause");
    let btn_w = theme::gold_button_width(ui, &play_label)
        .max(theme::gold_button_width(ui, &stop_label))
        .max(theme::gold_button_width(ui, &pause_label));
    match state.radio.status {
        RadioStatus::Playing => {
            if theme::gold_button_sized(ui, format!("{pause_label}##radio_pause"), [btn_w, 0.0]) {
                // pause() never locks STATE; the status is written
                // optimistically here (the audio thread deliberately goes
                // quiet while paused), same pattern as Stop below.
                player::pause();
                state.radio.status = RadioStatus::Paused;
            }
            ui.same_line_with_spacing(0.0, 8.0);
            stop_button(ui, state, &stop_label, btn_w);
        }
        RadioStatus::Paused => {
            if theme::gold_button_sized(ui, format!("{play_label}##radio_play"), [btn_w, 0.0]) {
                player::resume();
                state.radio.status = RadioStatus::Playing;
            }
            ui.same_line_with_spacing(0.0, 8.0);
            stop_button(ui, state, &stop_label, btn_w);
        }
        RadioStatus::Connecting | RadioStatus::Stalled => {
            stop_button(ui, state, &stop_label, btn_w);
        }
        _ => {
            let resumable = state.radio.current.clone().or_else(|| {
                state
                    .config
                    .radio
                    .last_station
                    .as_ref()
                    .map(station_from_saved)
            });
            match resumable {
                Some(station) => {
                    if theme::gold_button_sized(
                        ui,
                        format!("{play_label}##radio_play"),
                        [btn_w, 0.0],
                    ) {
                        start_play(state, station);
                    }
                }
                None => dim_button(ui, &format!("{play_label}##radio_play"), btn_w),
            }
        }
    }

    ui.same_line_with_spacing(0.0, 16.0);
    ui.align_text_to_frame_padding();
    ui.text_colored(theme::MUTED, t("radio.volume"));
    ui.same_line_with_spacing(0.0, 8.0);
    // Reserve the combat-duck checkbox's footprint (drawn at 0.85 scale)
    // before clamping the slider, so the checkbox never falls off the bar.
    let duck_label = t("radio.duck_in_combat");
    let duck_w = ui.calc_text_size(&duck_label)[0] * 0.85 + theme::control_height(ui) + 10.0;
    let slider_w = (ui.content_region_avail()[0] - pad - duck_w).clamp(90.0, 220.0);
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

    ui.same_line_with_spacing(0.0, 10.0);
    ui.set_window_font_scale(0.85);
    let mut duck = state.config.radio.duck_in_combat;
    if ui.checkbox(format!("{duck_label}##radio_duck"), &mut duck) {
        state.config.radio.duck_in_combat = duck;
        crate::ui::save_config_detached(state);
    }
    ui.set_window_font_scale(1.0);
    ui.set_cursor_screen_pos([origin[0], origin[1] + bar_h]);
}

/// The shared Stop button: signal-only stop + optimistic status write. This
/// click handler runs under the frame's STATE guard, where the joining
/// `player::stop()` could never finish its join — hence `request_stop`.
fn stop_button(ui: &Ui, state: &mut AddonState, label: &str, w: f32) {
    if theme::gold_button_sized(ui, format!("{label}##radio_stop"), [w, 0.0]) {
        player::request_stop();
        state.radio.status = RadioStatus::Stopped;
    }
}

/// Real equalizer in the bar's background: 24 low-alpha gold bars driven by
/// the decoded audio via `player::eq_levels()` (one call per frame — the
/// smoothing lives there). Skipped entirely while idle, so a silent bar costs
/// nothing and shows nothing.
fn eq_bars(dl: &DrawListMut, origin: [f32; 2], w: f32, bar_h: f32) {
    let levels = player::eq_levels();
    if levels.iter().all(|l| *l < 0.004) {
        return;
    }
    let pad = 8.0;
    let gap = 2.0;
    let n = player::EQ_BANDS as f32;
    let bar_w = ((w - pad * 2.0) - gap * (n - 1.0)) / n;
    if bar_w < 1.0 {
        return;
    }
    let base = origin[1] + bar_h - 2.0;
    let max_h = bar_h - 4.0;
    // Theme gold at low alpha — "slight transparency", text stays readable.
    let fill = [theme::GOLD[0], theme::GOLD[1], theme::GOLD[2], 0.14];
    for (i, level) in levels.iter().enumerate() {
        let h = max_h * level.clamp(0.0, 1.0);
        if h < 0.5 {
            continue;
        }
        let x = origin[0] + pad + i as f32 * (bar_w + gap);
        dl.add_rect([x, base - h], [x + bar_w, base], fill)
            .filled(true)
            .rounding(1.0)
            .build();
    }
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
        RadioStatus::Paused => (t("radio.status.paused"), theme::MUTED),
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
        // A rehydrated favorite has no directory vote count.
        votes: 0,
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
        tf(
            "fmt.radio_bitrate",
            &[("kbps", bitrate.to_string().as_str())],
        )
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
        .join(" - ")
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
    // Accumulate per-char widths instead of re-measuring the growing prefix:
    // O(n) instead of O(n²) — this runs per row, per frame, in a game overlay.
    // Summed advances ignore kerning, which the bundled fonts do not use.
    let ell_w = ui.calc_text_size("…")[0];
    let mut s = String::new();
    let mut w = 0.0;
    let mut buf = [0u8; 4];
    for ch in text.chars() {
        let cw = ui.calc_text_size(ch.encode_utf8(&mut buf))[0];
        if w + cw + ell_w > max_w {
            break;
        }
        w += cw;
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
        assert_eq!(join_meta(["DE", "MP3", "128 kbps"]), "DE - MP3 - 128 kbps");
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
