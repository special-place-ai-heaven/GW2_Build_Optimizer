//! Choya DJ corner sprite + radio garnish (ON AIR badge, EQ bars) from the
//! `choya_radio.png` atlas (1536x1024 RGBA, embedded).
//!
//! Self-contained: own embedded-texture helper (mirroring `theme::embedded_tex`),
//! own rect tables, draw-list blits only — nothing here is interactive. Call
//! [`draw_corner_choya`] from INSIDE the stations child window so the garnish
//! clips to the list area and never paints over the player bar below it.

use std::sync::atomic::{AtomicU32, Ordering};

use nexus::imgui::{DrawListMut, TextureId, Ui};

use crate::radio::RadioStatus;
use crate::state::AddonState;

const SHEET_W: f32 = 1536.0;
const SHEET_H: f32 = 1024.0;

// Pixel rects on `choya_radio.png`: x, y, w, h. Every rect below was verified
// frame-by-frame against a rendered montage of the atlas (2026-08-31). The
// auto-measured DANCE[0], ONAIR, ZZZ and EQ_WARM rects were wrong and are
// corrected here — see the comments on each.
const RADIO_DJ_IDLE: [[f32; 4]; 5] = [
    [5.0, 10.0, 212.0, 200.0],
    [221.0, 0.0, 209.0, 210.0],
    [650.0, 13.0, 214.0, 197.0],
    [1131.0, 10.0, 197.0, 201.0],
    [438.0, 10.0, 206.0, 200.0],
];
const RADIO_DJ_MIX: [[f32; 4]; 5] = [
    [10.0, 224.0, 182.0, 190.0],
    [204.0, 230.0, 199.0, 186.0],
    [623.0, 226.0, 194.0, 184.0],
    [829.0, 222.0, 175.0, 187.0],
    [418.0, 223.0, 187.0, 189.0],
];
/// Frame 0 was auto-measured as 182x348 — two stacked sprites merged into one
/// box. Re-measured to the upper dancer's alpha bounds.
const RADIO_DANCE: [[f32; 4]; 6] = [
    [12.0, 428.0, 181.0, 175.0],
    [194.0, 431.0, 180.0, 179.0],
    [528.0, 439.0, 164.0, 165.0],
    [706.0, 422.0, 163.0, 176.0],
    [874.0, 414.0, 132.0, 144.0],
    [1196.0, 230.0, 161.0, 176.0],
];
const RADIO_SLEEP: [[f32; 4]; 4] = [
    [804.0, 853.0, 154.0, 130.0],
    [969.0, 871.0, 132.0, 108.0],
    [1113.0, 861.0, 123.0, 121.0],
    [1247.0, 854.0, 164.0, 131.0],
];
const RADIO_MUTED: [f32; 4] = [694.0, 614.0, 126.0, 158.0];
const RADIO_LOVE: [f32; 4] = [357.0, 612.0, 163.0, 172.0];
/// Red ON AIR speech bubble. The auto-measurer merged it into the warm-EQ
/// block; the rect it proposed ([1314,683,63,59]) is actually the zZZ glyphs.
const RADIO_ONAIR: [f32; 4] = [1386.0, 668.0, 134.0, 82.0];
/// Blue zZZ glyphs. The proposed rect ([1286,690,30,30]) was a small heart.
const RADIO_ZZZ: [f32; 4] = [1313.0, 682.0, 66.0, 60.0];
const RADIO_EQ_COOL: [f32; 4] = [1060.0, 610.0, 215.0, 59.0];
/// Warm strip only; the proposed 231x120 box also swallowed the hearts, the
/// zZZ and the ON AIR badge below it.
const RADIO_EQ_WARM: [f32; 4] = [1291.0, 620.0, 228.0, 56.0];
const RADIO_HEART: [f32; 4] = [1227.0, 680.0, 52.0, 51.0];
const RADIO_NOTE_GOLD: [f32; 4] = [854.0, 683.0, 35.0, 60.0];

/// Corner sprite footprint. The stations child reserves nothing for it — the
/// DJ floats over the list's bottom-right like a watermark.
const CHOYA_SIZE: f32 = 110.0;
const MARGIN: f32 = 8.0;

// Frame pacing at the overlay's ~60 fps render rate (same assumption as the
// `frame_count` animations in `theme.rs`).
const SLEEP_STEP: u32 = 30; // ~2 fps
const DANCE_STEP: u32 = 8; // ~8 fps
const DJ_STEP: u32 = 10; // ~6 fps
const MIX_STEP: u32 = 9;
/// A mix burst starts every ~8 s and runs the 5 MIX frames once.
const MIX_PERIOD: u32 = 480;
const MIX_LEN: u32 = MIX_STEP * RADIO_DJ_MIX.len() as u32;
/// Heart flash duration after a favorite is added (~1 s).
const LOVE_FRAMES: u32 = 60;

/// Frame index of the last favorite-add, 0 = never. `frame_count` is already
/// nonzero by the first rendered frame, so 0 is a safe sentinel.
static LOVE_FLASH: AtomicU32 = AtomicU32::new(0);

/// Note a favorite was just added; the DJ hugs a heart for ~1 s.
pub fn flash_love(now_frames: u32) {
    LOVE_FLASH.store(now_frames.max(1), Ordering::Relaxed);
}

fn love_flash_active(flash: u32, now: u32) -> bool {
    flash != 0 && now.wrapping_sub(flash) < LOVE_FRAMES
}

/// Draw the DJ choya in the tab corner, state-driven.
pub fn draw_corner_choya(ui: &Ui, state: &AddonState) {
    let Some(tid) = radio_sheet() else {
        return;
    };
    let dl = ui.get_window_draw_list();
    let wp = ui.window_pos();
    let ws = ui.window_size();
    let center = [
        wp[0] + ws[0] - CHOYA_SIZE * 0.5 - MARGIN,
        wp[1] + ws[1] - CHOYA_SIZE * 0.5 - MARGIN,
    ];
    let t = ui.frame_count() as u32;
    let playing = state.radio.status == RadioStatus::Playing;
    let muted = playing && state.config.radio.volume_percent == 0;

    if love_flash_active(LOVE_FLASH.load(Ordering::Relaxed), t) {
        blit(&dl, tid, center, CHOYA_SIZE, RADIO_LOVE, 1.0);
        // A little heart drifts up and fades over the flash.
        let age = t.wrapping_sub(LOVE_FLASH.load(Ordering::Relaxed)) as f32;
        let rise = age * 0.6;
        let fade = (1.0 - age / LOVE_FRAMES as f32).clamp(0.0, 1.0);
        blit(
            &dl,
            tid,
            [
                center[0] + CHOYA_SIZE * 0.34,
                center[1] - CHOYA_SIZE * 0.42 - rise,
            ],
            18.0,
            RADIO_HEART,
            fade,
        );
        if playing {
            draw_playing_garnish(&dl, tid, center, t);
        }
        return;
    }

    match state.radio.status {
        RadioStatus::Connecting | RadioStatus::Stalled => {
            let i = (t / DANCE_STEP) as usize % RADIO_DANCE.len();
            blit(&dl, tid, center, CHOYA_SIZE, RADIO_DANCE[i], 1.0);
        }
        RadioStatus::Playing if muted => {
            blit(&dl, tid, center, CHOYA_SIZE, RADIO_MUTED, 1.0);
            draw_playing_garnish(&dl, tid, center, t);
        }
        RadioStatus::Playing => {
            let cycle = t % MIX_PERIOD;
            let bob = (t as f32 * 0.06).sin() * 2.5;
            let c = [center[0], center[1] + bob];
            if cycle < MIX_LEN {
                let i = (cycle / MIX_STEP) as usize % RADIO_DJ_MIX.len();
                blit(&dl, tid, c, CHOYA_SIZE, RADIO_DJ_MIX[i], 1.0);
            } else {
                let i = (t / DJ_STEP) as usize % RADIO_DJ_IDLE.len();
                blit(&dl, tid, c, CHOYA_SIZE, RADIO_DJ_IDLE[i], 1.0);
            }
            // A gold note drifts up past the deck now and then.
            let orbit = (t % 240) as f32 / 240.0;
            let note_a = (1.0 - (orbit * 2.0 - 1.0).abs()).clamp(0.0, 0.8);
            blit(
                &dl,
                tid,
                [
                    center[0] - CHOYA_SIZE * 0.52,
                    center[1] - CHOYA_SIZE * (0.1 + orbit * 0.45),
                ],
                16.0,
                RADIO_NOTE_GOLD,
                note_a,
            );
            draw_playing_garnish(&dl, tid, center, t);
        }
        // Idle, Stopped, DeviceLost and Error all read as "off the decks".
        _ => {
            let i = (t / SLEEP_STEP) as usize % RADIO_SLEEP.len();
            blit(&dl, tid, center, CHOYA_SIZE, RADIO_SLEEP[i], 1.0);
            let drift = (t as f32 * 0.03).sin();
            blit(
                &dl,
                tid,
                [
                    center[0] + CHOYA_SIZE * 0.30,
                    center[1] - CHOYA_SIZE * 0.44 + drift * 3.0,
                ],
                26.0,
                RADIO_ZZZ,
                0.55 + 0.25 * drift,
            );
        }
    }
}

/// ON AIR badge + pulsing EQ strip, sitting on the same baseline as the DJ so
/// they read as one bottom-edge garnish just above the player bar. The EQ is
/// a sprite, not a real visualizer — it warms up during the mix burst.
fn draw_playing_garnish(dl: &DrawListMut, tid: TextureId, choya_center: [f32; 2], t: u32) {
    let base_y = choya_center[1] + CHOYA_SIZE * 0.36;
    let badge_x = choya_center[0] - CHOYA_SIZE * 0.5 - 36.0;
    let flash = 0.85 + 0.15 * (t as f32 * 0.08).sin();
    blit(dl, tid, [badge_x, base_y], 56.0, RADIO_ONAIR, flash);

    let pulse = (t as f32 * 0.09).sin();
    let eq = if t % MIX_PERIOD < MIX_LEN {
        RADIO_EQ_WARM
    } else {
        RADIO_EQ_COOL
    };
    blit(
        dl,
        tid,
        [badge_x - 36.0 - 45.0, base_y + 6.0],
        90.0 * (1.0 + 0.04 * pulse),
        eq,
        0.60 + 0.22 * pulse,
    );
}

fn radio_sheet() -> Option<TextureId> {
    embedded_tex(
        "GW2BO_CHOYA_RADIO",
        include_bytes!("../../assets/choya_radio.png"),
    )
}

/// Private mirror of `theme::embedded_tex` — same Nexus texture cache, own key.
fn embedded_tex(key: &'static str, bytes: &'static [u8]) -> Option<TextureId> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        nexus::texture::get_texture_or_create_from_memory(key, bytes)
    }))
    .ok()
    .flatten()
    .map(|t| t.id())
}

/// Half-pixel inset like `theme::sheet_uv`, so bilinear sampling never bleeds
/// a neighboring sprite in.
fn sheet_uv(frame: [f32; 4]) -> ([f32; 2], [f32; 2]) {
    let [x, y, w, h] = frame;
    let inset = 0.5;
    let u0 = ((x + inset) / SHEET_W).clamp(0.0, 1.0);
    let v0 = ((y + inset) / SHEET_H).clamp(0.0, 1.0);
    let u1 = ((x + w - inset) / SHEET_W).clamp(0.0, 1.0);
    let v1 = ((y + h - inset) / SHEET_H).clamp(0.0, 1.0);
    (
        [u0, v0],
        [u1.max(u0 + f32::EPSILON), v1.max(v0 + f32::EPSILON)],
    )
}

/// Aspect-preserving blit centered on `center`; `size` bounds the longer side.
fn blit(
    dl: &DrawListMut,
    tid: TextureId,
    center: [f32; 2],
    size: f32,
    frame: [f32; 4],
    alpha: f32,
) {
    let [_, _, w, h] = frame;
    let aspect = (w / h).max(0.01);
    let (dw, dh) = if aspect > 1.0 {
        (size, size / aspect)
    } else {
        (size * aspect, size)
    };
    let pmin = [center[0] - dw * 0.5, center[1] - dh * 0.5];
    let pmax = [center[0] + dw * 0.5, center[1] + dh * 0.5];
    let (uv0, uv1) = sheet_uv(frame);
    dl.add_image(tid, pmin, pmax)
        .uv_min(uv0)
        .uv_max(uv1)
        .col([1.0, 1.0, 1.0, alpha.clamp(0.0, 1.0)])
        .build();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_sheet_uvs_stay_on_atlas() {
        let singles = [
            RADIO_MUTED,
            RADIO_LOVE,
            RADIO_ONAIR,
            RADIO_ZZZ,
            RADIO_EQ_COOL,
            RADIO_EQ_WARM,
            RADIO_HEART,
            RADIO_NOTE_GOLD,
        ];
        for frame in RADIO_DJ_IDLE
            .into_iter()
            .chain(RADIO_DJ_MIX)
            .chain(RADIO_DANCE)
            .chain(RADIO_SLEEP)
            .chain(singles)
        {
            let (a, b) = sheet_uv(frame);
            assert!(a[0] >= 0.0 && a[1] >= 0.0, "{frame:?} {a:?}");
            assert!(b[0] <= 1.0 && b[1] <= 1.0, "{frame:?} {b:?}");
            assert!(b[0] > a[0] && b[1] > a[1], "{frame:?}");
        }
    }

    #[test]
    fn radio_rects_stay_inside_the_sheet_in_pixels() {
        let singles = [
            RADIO_MUTED,
            RADIO_LOVE,
            RADIO_ONAIR,
            RADIO_ZZZ,
            RADIO_EQ_COOL,
            RADIO_EQ_WARM,
            RADIO_HEART,
            RADIO_NOTE_GOLD,
        ];
        for [x, y, w, h] in RADIO_DJ_IDLE
            .into_iter()
            .chain(RADIO_DJ_MIX)
            .chain(RADIO_DANCE)
            .chain(RADIO_SLEEP)
            .chain(singles)
        {
            assert!(x >= 0.0 && y >= 0.0);
            assert!(x + w <= SHEET_W, "x+w {} past sheet", x + w);
            assert!(y + h <= SHEET_H, "y+h {} past sheet", y + h);
            assert!(w > 0.0 && h > 0.0);
        }
    }

    #[test]
    fn love_flash_lasts_about_a_second() {
        assert!(!love_flash_active(0, 100)); // never flashed
        assert!(love_flash_active(100, 100));
        assert!(love_flash_active(100, 100 + LOVE_FRAMES - 1));
        assert!(!love_flash_active(100, 100 + LOVE_FRAMES));
    }

    #[test]
    fn mix_burst_fits_inside_its_period() {
        const { assert!(MIX_LEN < MIX_PERIOD) };
        // Every MIX frame is reachable inside a burst.
        assert_eq!((MIX_LEN - 1) / MIX_STEP, RADIO_DJ_MIX.len() as u32 - 1);
    }
}
