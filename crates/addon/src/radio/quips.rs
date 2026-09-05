//! AI quips: Choya reacts to the current song via the configured LLM.
//!
//! Opt-in (`radio.ai_quips` in config). `tick()` is called once per frame
//! from the player bar while Playing; it fetches one batch of 5 quips per
//! song title through the normal `spawn_worker` pipeline, under a hard
//! budget (max 30 calls/day, min 90 s between calls — a failed request
//! still consumes its slot, which is the backoff). `pick()` hands a quip
//! to the DJ bubble as an override for its canned tables.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// True while a quip request worker is running. Guards against one fetch
/// per frame; cleared by the worker on every exit path.
static IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Latest sanitized batch: (quips, song title they were generated for).
static QUEUE: Mutex<(Vec<String>, String)> = Mutex::new((Vec::new(), String::new()));

/// Call budget: (unix day, calls today, last call unix secs).
static BUDGET: Mutex<(u64, u32, u64)> = Mutex::new((0, 0, 0));

const MAX_CALLS_PER_DAY: u32 = 30;
const MIN_CALL_GAP_SECS: u64 = 90;

const PROMPT_TEMPLATE: &str = "You are Choya, a melon-bodied walking cactus from the Crystal Desert who DJs inside a radio overlay. Per the lore you grumble more than you speak, use simple tools, are famously aggressive, love dancing, shiny things and coconuts, and your village stays peaceful only because you kick troublemakers off the mesa. You flip moods without warning: roastmaster (tease the listener's taste; yours is superior), needled (spiky, threatening to burst, grumbling), smitten (overwhelmed, wants headpats), or unhinged gamer-brain (absurd GW2 references: quaggan, skritt, Commander, raptor, jade bot). You secretly adore the listener. Funny beats clever; cute beats cool.\n\nNow playing: {title} on {station}. Tags: {tags}. Energy: {energy} (0 = sleepy, 1 = maximum banger).\n\nWrite 5 quips reacting to this track, matched to the energy, mixing at least two moods. Output exactly 5 lines, one quip per line, max 6 words per line, plain ASCII only. No numbering, no quotes, no emoji, no extra text before or after.";

/// A fetched AI quip for this slot, or `None` when the queue is empty
/// (or poisoned). The bubble renderer falls back to its canned tables.
pub fn pick(seed: u32) -> Option<String> {
    let queue = QUEUE.lock().ok()?;
    if queue.0.is_empty() {
        return None;
    }
    queue.0.get(seed as usize % queue.0.len()).cloned()
}

/// Per-frame driver, called from the player bar while it renders (the call
/// site is the visibility gate). Kicks off at most one fetch per song
/// title, budget permitting.
pub fn tick(state: &crate::state::AddonState, bass: f32) {
    if !state.config.radio.ai_quips {
        // Opt-out also stops DISPLAY, not just fetching: drop any batch so
        // pick() reverts to the canned tables immediately.
        if let Ok(mut queue) = QUEUE.lock() {
            if !queue.0.is_empty() {
                *queue = (Vec::new(), String::new());
            }
        }
        return;
    }
    if state.radio.status != crate::radio::RadioStatus::Playing {
        return;
    }

    let title = match state.radio.now_playing.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(t) if !t.is_empty() => t.clone(),
            _ => return,
        },
        Err(_) => return,
    };

    // Already have a batch for this exact song — nothing to do. A batch for
    // a DIFFERENT song is stale: clear it so the canned tables cover the gap
    // instead of last song's lines lingering (the fetch below replaces it).
    match QUEUE.lock() {
        Ok(mut queue) => {
            if !queue.0.is_empty() {
                if queue.1 == title {
                    return;
                }
                queue.0.clear();
            }
        }
        Err(_) => return,
    }

    if IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return; // a fetch is already running
    }

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match BUDGET.lock() {
        Ok(mut budget) => {
            // Counter and last-call stamp advance BEFORE the request: a
            // failed request still consumes the slot — that is the backoff.
            if !budget_admit(&mut budget, now_secs) {
                drop(budget);
                IN_FLIGHT.store(false, Ordering::SeqCst);
                return;
            }
        }
        Err(_) => {
            IN_FLIGHT.store(false, Ordering::SeqCst);
            return;
        }
    }

    let config = state.config.clone();
    let addon_dir = state.addon_dir.clone();
    let (station, tags) = state
        .radio
        .current
        .as_ref()
        .map(|s| (s.name.clone(), s.tags.clone()))
        .unwrap_or_default();
    let energy = if bass > 0.32 {
        "banger"
    } else if bass > 0.18 {
        "groove"
    } else {
        "chill"
    };
    let prompt = PROMPT_TEMPLATE
        .replace("{title}", &title)
        .replace("{station}", &station)
        .replace("{tags}", &tags)
        .replace("{energy}", energy);

    let spawned = state.spawn_worker("radio-quip", move |token| {
        // Drop guard so a panic anywhere below (create_client, generate,
        // sanitize) cannot leave IN_FLIGHT stuck true for the session.
        struct ClearInFlight;
        impl Drop for ClearInFlight {
            fn drop(&mut self) {
                IN_FLIGHT.store(false, Ordering::SeqCst);
            }
        }
        let _clear = ClearInFlight;
        if !token.is_cancelled() {
            let result = gw2_optimizer::llm::create_client(&config, &addon_dir)
                .and_then(|client| client.generate(&prompt));
            if let Ok(text) = result {
                let quips = sanitize_batch(&text);
                if !quips.is_empty() {
                    if let Ok(mut queue) = QUEUE.lock() {
                        *queue = (quips, title);
                    }
                }
            }
        }
    });
    if !spawned {
        IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}

/// Admit or reject one call under the budget. Resets the daily counter on
/// day rollover; on admit, stamps the call immediately (pre-request).
fn budget_admit(budget: &mut (u64, u32, u64), now_secs: u64) -> bool {
    let day = now_secs / 86_400;
    if day != budget.0 {
        budget.0 = day;
        budget.1 = 0;
    }
    if budget.1 >= MAX_CALLS_PER_DAY {
        return false;
    }
    if now_secs.saturating_sub(budget.2) < MIN_CALL_GAP_SECS {
        return false;
    }
    budget.1 += 1;
    budget.2 = now_secs;
    true
}

/// LLM output -> at most 5 clean quip lines: list markers and surrounding
/// quotes stripped, non-ASCII and control chars dropped (the game font
/// renders them as '?'), each line capped at 40 chars, empties removed.
fn sanitize_batch(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| {
            let line = line
                .trim()
                .trim_start_matches(|c: char| {
                    c.is_ascii_digit() || matches!(c, '.' | '-' | ')' | '*')
                })
                .trim()
                .trim_matches(|c| c == '"' || c == '\'');
            let clean: String = line
                .chars()
                .filter(|c| c.is_ascii() && !c.is_ascii_control())
                .take(40)
                .collect();
            let clean = clean.trim().to_string();
            if clean.is_empty() {
                None
            } else {
                Some(clean)
            }
        })
        .take(5)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_list_markers_and_quotes() {
        let raw = "1. Skritt approved this track\n2) Headpats please Commander\n- Quaggan would cry\n* Raptor noises intensify\n\"My taste is superior\"";
        assert_eq!(
            sanitize_batch(raw),
            vec![
                "Skritt approved this track",
                "Headpats please Commander",
                "Quaggan would cry",
                "Raptor noises intensify",
                "My taste is superior",
            ]
        );
    }

    #[test]
    fn sanitize_drops_non_ascii_and_control_chars() {
        let quips = sanitize_batch("caf\u{e9} vibes \u{2764} only\nplain line\ttab");
        assert_eq!(quips, vec!["caf vibes  only", "plain linetab"]);
        for q in &quips {
            assert!(q.is_ascii());
        }
    }

    #[test]
    fn sanitize_caps_lines_and_length() {
        let long = "a".repeat(80);
        let raw = format!("one\ntwo\nthree\nfour\n{long}\nsix\nseven");
        let quips = sanitize_batch(&raw);
        assert_eq!(quips.len(), 5);
        assert_eq!(quips[4].chars().count(), 40);
    }

    #[test]
    fn sanitize_drops_empty_lines() {
        assert_eq!(sanitize_batch("\n   \n1.\n\"\"\nreal quip\n"), vec!["real quip"]);
    }

    #[test]
    fn budget_enforces_daily_cap_and_gap() {
        let mut b = (0u64, 0u32, 0u64);
        let day_start = 86_400 * 20_000;
        // First call passes and stamps.
        assert!(budget_admit(&mut b, day_start));
        assert_eq!(b, (20_000, 1, day_start));
        // Too soon: 89 s later is rejected, 90 s passes.
        assert!(!budget_admit(&mut b, day_start + 89));
        assert!(budget_admit(&mut b, day_start + 90));
        // Burn the rest of the daily budget.
        let mut t = day_start + 90;
        for _ in 2..MAX_CALLS_PER_DAY {
            t += MIN_CALL_GAP_SECS;
            assert!(budget_admit(&mut b, t));
        }
        assert_eq!(b.1, MAX_CALLS_PER_DAY);
        // Cap reached: even a well-spaced call is rejected.
        assert!(!budget_admit(&mut b, t + 10_000));
    }

    #[test]
    fn budget_resets_on_day_rollover() {
        let day = 86_400 * 20_000;
        let mut b = (20_000u64, MAX_CALLS_PER_DAY, day + 80_000);
        // Same day, cap reached: rejected.
        assert!(!budget_admit(&mut b, day + 80_100));
        // Next day: counter resets, call admitted.
        let next_day = day + 86_400 + 100;
        assert!(budget_admit(&mut b, next_day));
        assert_eq!(b, (20_001, 1, next_day));
    }
}
