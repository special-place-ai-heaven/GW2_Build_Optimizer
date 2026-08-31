//! Playback core: stream-download + icy-metadata + rodio on a dedicated
//! tokio runtime (1 worker), owned by this module behind a
//! `Mutex<Option<...>>` so `shutdown` can tear it down deterministically.
//!
//! ## Ownership
//!
//! Three statics own everything:
//! - [`RUNTIME`] owns the tokio runtime (created lazily on first play; taken
//!   and shut down by [`shutdown`]).
//! - [`SESSION`] owns the live station session: its stop flag, the shared
//!   sink handle for [`set_volume`], and the audio-owner thread's JoinHandle.
//! - [`VOLUME_PERCENT`] mirrors the config volume so a slider move during
//!   tune-in is never lost.
//!
//! The audio-owner thread (`gw2bo-radio-audio`) owns the rodio
//! `MixerDeviceSink` + `Player` for the lifetime of one station session and is
//! the only thread that touches rodio/cpal — never the render thread, never a
//! tokio worker. It also owns every status write for its session, which keeps
//! the public entry points safe to call from the render thread even while the
//! STATE mutex is held (they never lock STATE on the calling thread).
//!
//! ## Locking discipline
//!
//! [`play`] and [`set_volume`] never block: safe from any thread, including
//! inside `with_state`. [`stop`]'s bounded join can cost up to its ~1s budget
//! if the caller holds STATE while the audio thread is waiting on it — the
//! join is bounded precisely so that worst case is a hitch, never a deadlock.
//! [`toggle`] reads STATE on the calling thread and must only be called from
//! threads that do not hold it (the keybind handler path).

use std::io::{Read, Seek};
use std::net::ToSocketAddrs;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use gw2_core::config::SavedStation;
use icy_metadata::{IcyHeaders, IcyMetadataReader};
use rodio::{Decoder, DeviceSinkBuilder, Player};
use stream_download::http::{reqwest, HttpStream};
use stream_download::storage::bounded::BoundedStorageProvider;
use stream_download::storage::memory::MemoryStorageProvider;
use stream_download::{Settings, StreamDownload};

use super::{NowPlayingCell, RadioStatus, RbStation};

/// How many bytes to buffer before playback may start. At a typical 128 kbps
/// (16 KiB/s) this is ~4 s of audio — enough cushion against jitter without a
/// long tune-in delay (stream-download's 256 KiB default would mean ~16 s).
const PREFETCH_BYTES: u64 = 64 * 1024;

/// Size of the in-memory ring buffer holding the live stream. Must comfortably
/// exceed [`PREFETCH_BYTES`]; ~30 s of audio at 128 kbps.
const RING_BUFFER_BYTES: usize = 512 * 1024;

/// Cap on establishing the TCP+TLS connection to the station.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Cap on the connect + header-wait phase (`HttpStream::new`). Bounds ONLY the
/// tune-in handshake — the audio body is an infinite stream and must never be
/// subject to a request timeout.
const HEADER_TIMEOUT: Duration = Duration::from_secs(15);

/// How often the watchdog checks the sink for an unexpected stall.
const WATCHDOG_INTERVAL: Duration = Duration::from_millis(500);

/// How finely the watchdog polls the stop flag between sink checks, so
/// [`stop`] interrupts a session promptly.
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A reconnect is quiet exactly once per stall: after a reconnect the stream
/// must play this long before another stall earns a new attempt, so a
/// flapping stream errors out instead of hot-looping connect attempts.
const STALL_REARM: Duration = Duration::from_secs(10);

/// Bounded join budget for [`stop`].
const STOP_JOIN_BUDGET: Duration = Duration::from_secs(1);

/// Bounded join budget inside [`shutdown`]: 600ms join + 700ms runtime
/// shutdown stays well inside `state::UNLOAD_JOIN_BUDGET` (1500ms).
const SHUTDOWN_JOIN_BUDGET: Duration = Duration::from_millis(600);

/// Runtime teardown cap inside [`shutdown`].
const RUNTIME_SHUTDOWN_BUDGET: Duration = Duration::from_millis(700);

/// UI cap for stream titles and error messages (`chars`, never bytes).
const MAX_TITLE_CHARS: usize = 200;

/// The bounds rodio's decoder needs from the reader stack, as one nameable
/// trait so the two shapes (with/without ICY metadata) can be boxed.
trait StreamReader: Read + Seek + Send + Sync {}
impl<T: Read + Seek + Send + Sync> StreamReader for T {}

/// The dedicated playback runtime. Not `OnceLock`: [`shutdown`] must `take()`
/// it and `shutdown_timeout` it deterministically before the DLL unloads.
static RUNTIME: Mutex<Option<tokio::runtime::Runtime>> = Mutex::new(None);

/// The live station session, if any.
static SESSION: Mutex<Option<Session>> = Mutex::new(None);

/// Last requested volume percent; seeded from config on play so a slider move
/// during the connect phase still wins over the config snapshot.
static VOLUME_PERCENT: AtomicU8 = AtomicU8::new(60);

/// One station session. Owned by [`SESSION`]; the audio-owner thread holds
/// clones of the stop flag and the sink cell, never the session itself.
/// Monotone session generation: bumped by every [`start_session`] and
/// [`request_stop`]. A session thread may only write status/config while its
/// generation is still current - a detached predecessor's late writes (stale
/// `Playing`, wrong `last_station`) are discarded at the lock, where the
/// check and the write are atomic with respect to the successor's writes.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn still_current(my_gen: u64) -> bool {
    GENERATION.load(Ordering::Acquire) == my_gen
}

struct Session {
    /// Set by [`stop`]/[`play`] to end the session; polled by the audio thread.
    stop: Arc<AtomicBool>,
    /// Sink handle published by the audio thread once the device is open, so
    /// [`set_volume`] can reach the live sink without touching the thread.
    sink: Arc<Mutex<Option<Arc<Player>>>>,
    /// JOINed (bounded) by [`stop`]/[`shutdown`], or by the next session's
    /// audio thread when [`play`] replaces this session.
    handle: Option<std::thread::JoinHandle<()>>,
}

/// Start playing `station`, replacing any current stream.
///
/// Never blocks and never locks STATE on the calling thread: the old session
/// is signalled here but joined by the new audio-owner thread, and every
/// status write happens on that thread. Safe to call from inside `with_state`.
pub fn play(station: &RbStation) {
    start_session(station.clone());
}

/// Stop playback and release the audio device. Cheap to call when idle.
///
/// Signals the session flag, then JOINs the audio-owner thread (bounded ~1s);
/// the thread drops the sink/stream and writes `Stopped` as its last act.
/// Keybind/unload path ONLY - never call while STATE is held (the join waits
/// on a status write into that lock); the tab UI uses [`request_stop`].
pub fn stop() {
    stop_session(STOP_JOIN_BUDGET);
}

/// Signal-only stop for callers that may hold STATE (the tab's Stop button,
/// settings reset): raises the stop flag and invalidates the session
/// generation, so the dying thread's late status writes are discarded - the
/// caller owns the status from here. Never joins and never locks STATE; the
/// audio thread reaps itself (~50ms) and its handle is joined by the next
/// [`play`] or by [`shutdown`].
pub fn request_stop() {
    GENERATION.fetch_add(1, Ordering::AcqRel);
    if let Some(session) = lock_or_recover(&SESSION).as_ref() {
        session.stop.store(true, Ordering::Release);
    }
}

/// Keybind toggle: playing -> stop; otherwise re-tune the current/last station.
///
/// Reads STATE on the calling thread — keybind-handler path only; must not be
/// called while STATE is held (the tab UI calls [`play`]/[`stop`] directly).
pub fn toggle() {
    let snapshot = crate::state::with_state(|s| {
        let active = matches!(
            s.radio.status,
            RadioStatus::Playing | RadioStatus::Connecting
        );
        let station = s
            .radio
            .current
            .clone()
            .or_else(|| s.config.radio.last_station.as_ref().map(station_from_saved));
        (active, station)
    });
    match snapshot {
        Some((true, _)) => stop(),
        Some((false, Some(station))) => play(&station),
        // Nothing ever played and nothing saved — or the addon is unloading.
        _ => {}
    }
}

/// Output gain from 0-100 percent; log taper applied inside.
///
/// Applies to the live sink if any and always returns fast. Never locks STATE.
pub fn set_volume(percent: u8) {
    VOLUME_PERCENT.store(percent, Ordering::Release);
    let sink = lock_or_recover(&SESSION)
        .as_ref()
        .and_then(|s| lock_or_recover(&s.sink).clone());
    if let Some(sink) = sink {
        sink.set_volume(volume_gain(percent));
    }
}

/// Unload teardown: stop sink -> join audio-owner thread -> drop reader ->
/// shut the tokio runtime down (bounded). Called from `on_unload` BEFORE
/// worker cancellation; must never block long or panic.
pub fn shutdown() {
    stop_session(SHUTDOWN_JOIN_BUDGET);
    let runtime = lock_or_recover(&RUNTIME).take();
    if let Some(runtime) = runtime {
        runtime.shutdown_timeout(RUNTIME_SHUTDOWN_BUDGET);
    }
}

// ---------------------------------------------------------------------------
// Session lifecycle
// ---------------------------------------------------------------------------

fn start_session(station: RbStation) {
    let my_gen = GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    let mut guard = lock_or_recover(&SESSION);
    // Signal the old session but let the NEW audio thread join it: the caller
    // may be the render thread holding STATE, and the old thread may be
    // blocked on a status write into that very STATE.
    let old = guard.take();
    if let Some(old) = &old {
        old.stop.store(true, Ordering::Release);
    }
    let old_handle = old.and_then(|mut o| o.handle.take());

    let stop = Arc::new(AtomicBool::new(false));
    let sink: Arc<Mutex<Option<Arc<Player>>>> = Arc::new(Mutex::new(None));

    let t_stop = Arc::clone(&stop);
    let t_sink = Arc::clone(&sink);
    // Pin the module before spawn returns (same contract as spawn_worker):
    // if this thread is ever detached - a stop/shutdown join timing out on a
    // socket read - FreeLibrary must not unmap `.text` under it.
    let pin = crate::state::pin_addon_module();
    let spawned = std::thread::Builder::new()
        .name("gw2bo-radio-audio".to_string())
        .spawn(move || {
            // The audio thread runs across an ABI-adjacent boundary (cpal,
            // symphonia); a panic must die here, not take the process down.
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_session(station, my_gen, t_stop, t_sink, old_handle);
            }));
            if outcome.is_err() {
                radio_log("radio audio thread panicked; session abandoned".to_string());
                let _ = crate::state::with_state(|s| {
                    if still_current(my_gen) {
                        s.radio.status =
                            RadioStatus::Error("playback thread panicked".to_string());
                    }
                });
            }
            if let Some(handle) = pin {
                crate::state::exit_pinned_worker(handle);
            }
        });
    match spawned {
        Ok(handle) => {
            *guard = Some(Session {
                stop,
                sink,
                handle: Some(handle),
            });
        }
        Err(e) => {
            crate::state::undo_module_pin(pin);
            radio_log(format!("failed to spawn radio audio thread: {e}"));
        }
    }
}

fn stop_session(join_budget: Duration) {
    // Take the session out under a short lock, join with the lock released.
    let session = lock_or_recover(&SESSION).take();
    if let Some(mut session) = session {
        session.stop.store(true, Ordering::Release);
        if let Some(handle) = session.handle.take() {
            join_bounded(handle, join_budget);
        }
    }
}

/// Everything a station session does, on the audio-owner thread.
fn run_session(
    station: RbStation,
    my_gen: u64,
    stop: Arc<AtomicBool>,
    sink_cell: Arc<Mutex<Option<Arc<Player>>>>,
    old_handle: Option<std::thread::JoinHandle<()>>,
) {
    // Serialize sessions: the old thread writes its final status and releases
    // the audio device before this session opens its own.
    if let Some(handle) = old_handle {
        join_bounded(handle, STOP_JOIN_BUDGET);
    }
    if stop.load(Ordering::Acquire) {
        return; // replaced or stopped before we even started
    }

    // Connecting on entry; snapshot the now-playing cell and config volume.
    // The Arc is cloned out of STATE here, BEFORE the stream opens, so the
    // ICY callback never locks addon state.
    let Some(Some((now_playing, volume))) = crate::state::with_state(|s| {
        if !still_current(my_gen) {
            return None; // replaced before we even connected
        }
        s.radio.status = RadioStatus::Connecting;
        s.radio.current = Some(station.clone());
        if let Ok(mut np) = s.radio.now_playing.lock() {
            *np = None;
        }
        Some((
            Arc::clone(&s.radio.now_playing),
            s.config.radio.volume_percent,
        ))
    }) else {
        return; // addon is unloading, or a newer session owns the state
    };
    VOLUME_PERCENT.store(volume, Ordering::Release);

    let handle = match runtime_handle() {
        Ok(h) => h,
        Err(msg) => {
            set_error(my_gen, msg);
            return;
        }
    };

    // Open the output device. `log_on_drop(false)`: rodio would otherwise
    // write a drop notice to stderr, which is a black hole in-game. The
    // device-lost flag lives here, shared only with the error callback.
    let device_lost = Arc::new(AtomicBool::new(false));
    let lost = Arc::clone(&device_lost);
    let opened = DeviceSinkBuilder::from_default_device().and_then(|b| {
        b.with_error_callback(move |_err| {
            // cpal's callback thread: set a flag only, the watchdog translates
            // it to `DeviceLost`. Never lock addon state from here.
            lost.store(true, Ordering::Release);
        })
        .open_sink_or_fallback()
    });
    let mut device = match opened {
        Ok(d) => d,
        Err(e) => {
            set_error(my_gen, short_msg("no audio output device", &e.to_string()));
            return;
        }
    };
    device.log_on_drop(false);
    let player = Arc::new(Player::connect_new(device.mixer()));
    *lock_or_recover(&sink_cell) = Some(Arc::clone(&player));

    // Connect, buffer, decode, append.
    match open_and_append(&handle, &station, &player, &now_playing, &stop) {
        Ok(()) => {}
        Err(SessionEnd::Cancelled) => {
            finish_stopped(my_gen, &player, &sink_cell);
            return;
        }
        Err(SessionEnd::Failed(msg)) => {
            *lock_or_recover(&sink_cell) = None;
            set_error(my_gen, msg);
            return;
        }
    }
    // Apply the stored volume through the same path the UI slider uses (the
    // published sink cell), then let it sound.
    set_volume(VOLUME_PERCENT.load(Ordering::Acquire));
    player.play();

    // Playing: publish the station and persist it for the keybind toggle.
    // The click ping (directory usage policy) rides the same lock: going
    // through spawn_worker gives it the module pin, registry tracking, and
    // cancel token that a raw fire-and-forget thread lacks.
    let published = crate::state::with_state(|s| {
        if !still_current(my_gen) {
            return false;
        }
        s.radio.status = RadioStatus::Playing;
        s.radio.current = Some(station.clone());
        s.config.radio.last_station = Some(saved_from_station(&station));
        crate::ui::save_config_detached(s);
        if !station.stationuuid.is_empty() {
            let uuid = station.stationuuid.clone();
            let _ = s.spawn_worker("radio-click", move |_token| {
                crate::radio::directory::click(&uuid);
            });
        }
        true
    })
    .unwrap_or(false);
    if !published {
        finish_stopped(my_gen, &player, &sink_cell);
        return;
    }

    // Watchdog: the sink was primed above ("stream ever started" is latched by
    // construction), so an empty sink from here on is a stall, not warm-up.
    let mut last_reconnect: Option<Instant> = None;
    loop {
        let ticks = (WATCHDOG_INTERVAL.as_millis() / STOP_POLL_INTERVAL.as_millis()).max(1);
        for _ in 0..ticks {
            std::thread::sleep(STOP_POLL_INTERVAL);
            if stop.load(Ordering::Acquire) {
                finish_stopped(my_gen, &player, &sink_cell);
                return;
            }
        }
        if device_lost.load(Ordering::Acquire) {
            // cpal does not recover on its own; the user presses play to
            // reopen. Leave the session; the next play() reaps this thread.
            *lock_or_recover(&sink_cell) = None;
            let _ = crate::state::with_state(|s| {
                if still_current(my_gen) {
                    s.radio.status = RadioStatus::DeviceLost;
                }
            });
            return;
        }
        if player.empty() {
            // The decoder ran dry: the stream ended or the connection dropped.
            let live = crate::state::with_state(|s| {
                if !still_current(my_gen) {
                    return false;
                }
                s.radio.status = RadioStatus::Stalled;
                true
            })
            .unwrap_or(false);
            if !live {
                finish_stopped(my_gen, &player, &sink_cell);
                return;
            }
            if !stall_wants_reconnect(last_reconnect.map(|t| t.elapsed())) {
                *lock_or_recover(&sink_cell) = None;
                set_error(my_gen, "stream keeps stalling".to_string());
                return;
            }
            match open_and_append(&handle, &station, &player, &now_playing, &stop) {
                Ok(()) => {
                    player.play();
                    last_reconnect = Some(Instant::now());
                    let live = crate::state::with_state(|s| {
                        if !still_current(my_gen) {
                            return false;
                        }
                        s.radio.status = RadioStatus::Playing;
                        true
                    })
                    .unwrap_or(false);
                    if !live {
                        finish_stopped(my_gen, &player, &sink_cell);
                        return;
                    }
                }
                Err(SessionEnd::Cancelled) => {
                    finish_stopped(my_gen, &player, &sink_cell);
                    return;
                }
                Err(SessionEnd::Failed(msg)) => {
                    *lock_or_recover(&sink_cell) = None;
                    set_error(my_gen, msg);
                    return;
                }
            }
        }
    }
}

/// Why `open_and_append` (or the session) ended early.
enum SessionEnd {
    /// The stop flag was raised; exit quietly.
    Cancelled,
    /// Something broke; the message goes into `RadioStatus::Error`.
    Failed(String),
}

/// Connect to the station, buffer it, strip ICY metadata, decode, and append
/// to `player`. Used for both the initial tune-in and the stall reconnect.
fn open_and_append(
    handle: &tokio::runtime::Handle,
    station: &RbStation,
    player: &Player,
    now_playing: &NowPlayingCell,
    stop: &Arc<AtomicBool>,
) -> Result<(), SessionEnd> {
    let opened = open_stream(handle, station.stream_url(), now_playing, stop)?;

    // A live stream has no filename, so the format probe is primed from the
    // Content-Type; `with_seekable(false)` keeps symphonia from issuing the
    // `Seek` that breaks on an infinite HTTP stream.
    let mut builder = Decoder::builder()
        .with_data(opened.reader)
        .with_seekable(false);
    if let Some(mime) = &opened.content_type {
        builder = builder.with_mime_type(mime);
    }
    let decoder = builder
        .build()
        .map_err(|e| SessionEnd::Failed(short_msg("cannot decode stream", &e.to_string())))?;

    player.clear();
    player.append(decoder);
    Ok(())
}

/// The connected, buffered, ICY-stripped stream — everything `open_stream`
/// produces before any decoder or audio device is involved, so the network
/// path stays testable without a sink.
struct OpenedStream {
    reader: Box<dyn StreamReader>,
    content_type: Option<String>,
}

/// Connect to the stream URL, buffer it, and wrap ICY metadata handling.
fn open_stream(
    handle: &tokio::runtime::Handle,
    url: &str,
    now_playing: &NowPlayingCell,
    stop: &Arc<AtomicBool>,
) -> Result<OpenedStream, SessionEnd> {
    // Enter the runtime context for this whole construction sequence:
    // `tokio::time::timeout` (and other tokio primitives) bind the timer
    // driver at CONSTRUCTION, and these futures are built on the plain audio
    // thread before `block_on` runs — without this guard that construction
    // panics with "there is no reactor running" (shipped crash in 1.8.0).
    let _reactor = handle.enter();

    let parsed: reqwest::Url = url
        .parse()
        .map_err(|_| SessionEnd::Failed("invalid stream URL".to_string()))?;

    // The station URL is community-submitted directory data: refuse to dial
    // into the local network. Hostnames are resolved once here; DNS rebinding
    // after the check is accepted residual risk (the attacker controls
    // timing, not this addon).
    if stream_host_reserved(&parsed) {
        return Err(SessionEnd::Failed(
            "station points at a private address".to_string(),
        ));
    }

    let mut headers = reqwest::header::HeaderMap::new();
    // Ask the server to interleave ICY metadata into the stream.
    icy_metadata::add_icy_metadata_header(&mut headers);
    let client = reqwest::Client::builder()
        // radio-browser.info asks clients to identify themselves; icecast
        // servers occasionally reject UA-less requests too.
        .user_agent(concat!("GW2BuildOptimizer/", env!("CARGO_PKG_VERSION")))
        .default_headers(headers)
        // Bound the connect phase only. Deliberately NO blanket `.timeout()`:
        // the audio body is infinite, and a request timeout would kill
        // playback mid-song.
        .connect_timeout(CONNECT_TIMEOUT)
        // Icecast mounts redirect occasionally (http->https, CDN hops); ten
        // hops (reqwest's default) is a tunnel, three is a stream.
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .map_err(|e| SessionEnd::Failed(short_msg("http client", &e.to_string())))?;

    // `HttpStream::new` connects and waits for response headers; a station
    // that accepts the connection then withholds headers would hang forever,
    // so the header-wait is capped. The select keeps stop() prompt even
    // mid-handshake — dropping the future cancels the connect.
    let stream = block_on_cancellable(
        handle,
        stop,
        tokio::time::timeout(HEADER_TIMEOUT, HttpStream::new(client, parsed)),
    )
    .ok_or(SessionEnd::Cancelled)?
    .map_err(|_| SessionEnd::Failed("timed out waiting for stream headers".to_string()))?
    .map_err(|e| SessionEnd::Failed(short_msg("connect failed", &e.to_string())))?;

    let icy_headers = IcyHeaders::parse_from_headers(stream.headers());
    let content_type = stream
        .content_type()
        .as_ref()
        .map(|ct| format!("{}/{}", ct.r#type, ct.subtype));

    let settings = Settings::default().prefetch_bytes(PREFETCH_BYTES);
    let storage = BoundedStorageProvider::new(
        MemoryStorageProvider,
        NonZeroUsize::new(RING_BUFFER_BYTES).expect("ring buffer size is non-zero"),
    );
    let download = block_on_cancellable(
        handle,
        stop,
        StreamDownload::from_stream(stream, storage, settings),
    )
    .ok_or(SessionEnd::Cancelled)?
    .map_err(|e| SessionEnd::Failed(short_msg("buffering failed", &e.to_string())))?;

    // Wrap in the ICY reader only when the server actually interleaves
    // metadata; the callback writes into the shared cell, never into STATE.
    let reader: Box<dyn StreamReader> = match icy_headers.metadata_interval() {
        Some(metaint) => {
            let cell = Arc::clone(now_playing);
            Box::new(IcyMetadataReader::new(
                download,
                Some(metaint),
                move |meta| {
                    // Parse errors on one metadata block are transient — keep the
                    // last good title rather than blanking the display.
                    if let Ok(meta) = meta {
                        if let Some(raw) = meta.stream_title() {
                            let title = sanitize_title(raw);
                            if let Ok(mut np) = cell.lock() {
                                *np = if title.is_empty() { None } else { Some(title) };
                            }
                        }
                    }
                },
            ))
        }
        // No `icy-metaint`: a plain audio stream, pass it through untouched.
        None => Box::new(download),
    };

    Ok(OpenedStream {
        reader,
        content_type,
    })
}

/// Stop-flag exit path: drop the sink handle, write `Stopped`, return.
/// Stopping the sink drops the decoder, which drops the stream reader, which
/// cancels the background download task (`cancel_on_drop`).
/// True when the stream URL's host is (or resolves to) an address inside the
/// local network - loopback, RFC1918, link-local, ULA, unspecified. Literal
/// IPs are checked directly; hostnames get one blocking resolve (the OS
/// caches it for the connect that follows). An unresolvable host returns
/// false: the connect will fail with its own honest error.
fn stream_host_reserved(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return true;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return crate::news_art::ip_is_reserved(ip);
    }
    let port = url.port_or_known_default().unwrap_or(443);
    match (host, port).to_socket_addrs() {
        Ok(mut addrs) => addrs.any(|a| crate::news_art::ip_is_reserved(a.ip())),
        Err(_) => false,
    }
}

fn finish_stopped(my_gen: u64, player: &Player, sink_cell: &Arc<Mutex<Option<Arc<Player>>>>) {
    player.clear();
    *lock_or_recover(sink_cell) = None;
    let _ = crate::state::with_state(|s| {
        if still_current(my_gen) {
            s.radio.status = RadioStatus::Stopped;
        }
    });
}

fn set_error(my_gen: u64, msg: String) {
    radio_log(format!("radio: {msg}"));
    let _ = crate::state::with_state(|s| {
        if still_current(my_gen) {
            s.radio.status = RadioStatus::Error(msg);
        }
    });
}

// ---------------------------------------------------------------------------
// Plumbing
// ---------------------------------------------------------------------------

/// Lazily create the playback runtime and hand out a handle to it.
fn runtime_handle() -> Result<tokio::runtime::Handle, String> {
    let mut guard = lock_or_recover(&RUNTIME);
    if guard.is_none() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("gw2bo-radio-rt")
            .enable_all()
            .build()
            .map_err(|e| short_msg("audio runtime failed", &e.to_string()))?;
        *guard = Some(runtime);
    }
    Ok(guard
        .as_ref()
        .expect("runtime just created")
        .handle()
        .clone())
}

/// `block_on` a future on the playback runtime, racing it against the session
/// stop flag so [`stop`] interrupts even a mid-handshake session promptly.
/// `None` = cancelled (the future is dropped, aborting the work).
fn block_on_cancellable<F: std::future::Future>(
    handle: &tokio::runtime::Handle,
    stop: &Arc<AtomicBool>,
    fut: F,
) -> Option<F::Output> {
    let stop = Arc::clone(stop);
    handle.block_on(async move {
        tokio::select! {
            out = fut => Some(out),
            () = async {
                while !stop.load(Ordering::Acquire) {
                    tokio::time::sleep(STOP_POLL_INTERVAL).await;
                }
            } => None,
        }
    })
}

/// Poll-join a thread with a budget. On timeout the handle is dropped (the
/// thread finishes on its own shortly after) and the abandonment is logged —
/// a bounded hitch, never a deadlock.
fn join_bounded(handle: std::thread::JoinHandle<()>, budget: Duration) {
    let deadline = Instant::now() + budget;
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            radio_log("radio audio thread outlived its join budget; detaching".to_string());
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = handle.join(); // finished: reap; panics were caught inside
}

/// Recover a poisoned module mutex the same way `state::lock_state` does:
/// the data is flags and handles, all valid regardless of where a panic hit.
fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Diagnostics through nexus, mirroring `state::worker_log` (which is private
/// there): tests have no Nexus API table, so test builds go to stderr.
fn radio_log(message: String) {
    #[cfg(test)]
    eprintln!("[GW2BuildOpt] {}", message);
    #[cfg(not(test))]
    nexus::log::log(
        nexus::log::LogLevel::Warning,
        "GW2 Build Optimizer",
        message,
    );
}

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested)
// ---------------------------------------------------------------------------

/// The gain for a volume percent: a log taper over a 60 dB range
/// (`1000^(p/100 - 1)`). 0% is silence, 100% is stream level, 80% ~ -12 dB.
fn volume_gain(percent: u8) -> f32 {
    const DB_RATIO: f64 = 1000.0;
    match percent {
        0 => 0.0,
        p if p >= 100 => 1.0,
        p => ((f64::from(p) / 100.0 - 1.0) * DB_RATIO.ln()).exp() as f32,
    }
}

/// Whether a stall has earned its ONE quiet reconnect attempt. The first
/// stall always has; after a reconnect the stream must have played for
/// [`STALL_REARM`] before another stall earns a new attempt.
fn stall_wants_reconnect(since_last_reconnect: Option<Duration>) -> bool {
    since_last_reconnect.is_none_or(|d| d >= STALL_REARM)
}

/// UI-safe stream title: control characters stripped, capped at
/// [`MAX_TITLE_CHARS`] via `chars()` (never byte slicing), trimmed.
fn sanitize_title(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .take(MAX_TITLE_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Status/log message with the detail capped UTF-8-safely.
fn short_msg(prefix: &str, detail: &str) -> String {
    let detail: String = detail.chars().take(MAX_TITLE_CHARS).collect();
    format!("{prefix}: {detail}")
}

/// Snapshot a directory row for persistence (`config.radio.last_station` /
/// favorites). `url` is the resolved stream URL so re-tuning never needs a
/// directory round-trip.
pub fn saved_from_station(station: &RbStation) -> SavedStation {
    SavedStation {
        stationuuid: station.stationuuid.clone(),
        name: station.name.clone(),
        url: station.stream_url().to_string(),
        favicon: station.favicon.clone(),
        codec: station.codec.clone(),
        bitrate: station.bitrate,
        countrycode: station.countrycode.clone(),
        tags: station.tags.clone(),
    }
}

/// Rehydrate a saved snapshot into a playable station row. The snapshot's
/// `url` was already resolved at save time, so it fills both URL fields; the
/// health flags are set playable — the save itself is the health evidence.
pub fn station_from_saved(saved: &SavedStation) -> RbStation {
    RbStation {
        stationuuid: saved.stationuuid.clone(),
        name: saved.name.clone(),
        url: saved.url.clone(),
        url_resolved: saved.url.clone(),
        favicon: saved.favicon.clone(),
        tags: saved.tags.clone(),
        countrycode: saved.countrycode.clone(),
        codec: saved.codec.clone(),
        bitrate: saved.bitrate,
        lastcheckok: 1,
        hls: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression for the v1.8.0 in-game crash: `tokio::time::timeout` grabs
    /// the timer driver at CONSTRUCTION, and `open_stream` builds it on the
    /// plain audio thread before `block_on` ever runs — panicking with
    /// "there is no reactor running". The `.invalid` TLD never resolves
    /// (RFC 2606), so the call must reach the tokio construction site and
    /// then fail the connect honestly — no sockets to real stream hosts.
    #[test]
    fn open_stream_off_runtime_thread_fails_cleanly_instead_of_panicking() {
        let handle = runtime_handle().expect("test runtime");
        let stop = Arc::new(AtomicBool::new(false));
        let np: NowPlayingCell = Arc::new(Mutex::new(None));
        let joined = std::thread::spawn(move || {
            open_stream(
                &handle,
                "http://gw2bo-no-such-host.invalid/stream",
                &np,
                &stop,
            )
            .err()
        })
        .join();
        let ended = joined.expect("open_stream must not panic off-runtime");
        assert!(
            matches!(ended, Some(SessionEnd::Failed(_))),
            "expected a clean connect failure"
        );
    }

    #[test]
    fn volume_gain_follows_the_log_curve() {
        assert_eq!(volume_gain(0), 0.0);
        assert_eq!(volume_gain(100), 1.0);
        assert_eq!(volume_gain(150), 1.0);
        let at_80 = volume_gain(80);
        assert!(
            (at_80 - 0.251).abs() < 0.001,
            "80% is about -12 dB: {at_80}"
        );
        let at_50 = volume_gain(50);
        assert!(
            (at_50 - 0.0316).abs() < 0.001,
            "50% is about -30 dB: {at_50}"
        );
        assert!(
            volume_gain(20) < at_50 && at_50 < at_80,
            "taper is monotonic"
        );
    }

    #[test]
    fn stall_policy_grants_the_first_reconnect_and_rearms_after_stable_play() {
        // The very first stall of a session always gets its one attempt.
        assert!(stall_wants_reconnect(None));
        // A stall right after a reconnect means the stream is bad — give up.
        assert!(!stall_wants_reconnect(Some(Duration::from_secs(3))));
        // After a stretch of stable playback the attempt is re-armed.
        assert!(stall_wants_reconnect(Some(STALL_REARM)));
        assert!(stall_wants_reconnect(Some(Duration::from_secs(3600))));
    }

    #[test]
    fn sanitize_title_strips_controls_and_caps_by_chars() {
        assert_eq!(
            sanitize_title("Artist \u{0} - \u{7} Title\r\n"),
            "Artist  -  Title"
        );
        assert_eq!(sanitize_title("   padded   "), "padded");
        assert_eq!(sanitize_title(""), "");
        // Multibyte input truncates on char boundaries, never mid-codepoint.
        let long: String = "ドイツ語".chars().cycle().take(500).collect();
        let cleaned = sanitize_title(&long);
        assert_eq!(cleaned.chars().count(), MAX_TITLE_CHARS);
        assert!(cleaned.starts_with('ド'));
    }

    #[test]
    fn station_round_trips_through_the_saved_snapshot() {
        let station = RbStation {
            stationuuid: "uuid-1".to_string(),
            name: "Groove Salad".to_string(),
            url: "https://example.com/playlist.pls".to_string(),
            url_resolved: "https://example.com/stream".to_string(),
            favicon: "https://example.com/icon.png".to_string(),
            tags: "ambient,chill".to_string(),
            countrycode: "US".to_string(),
            codec: "MP3".to_string(),
            bitrate: 128,
            lastcheckok: 1,
            hls: 0,
        };
        let saved = saved_from_station(&station);
        // The snapshot captures the RESOLVED url — the playable one.
        assert_eq!(saved.url, "https://example.com/stream");
        assert_eq!(saved.name, "Groove Salad");
        assert_eq!(saved.bitrate, 128);

        let back = station_from_saved(&saved);
        assert_eq!(back.stream_url(), "https://example.com/stream");
        assert_eq!(back.stationuuid, "uuid-1");
        assert_eq!(back.name, station.name);
        assert_eq!(back.codec, station.codec);
        assert_eq!(back.countrycode, station.countrycode);
        assert_eq!(back.tags, station.tags);
        assert_eq!(back.lastcheckok, 1, "a saved station is presumed playable");
        assert_eq!(back.hls, 0);
    }

    #[test]
    fn saved_station_with_empty_uuid_still_round_trips() {
        // A hand-entered or legacy save may lack a directory uuid; playback
        // (and the click-ping guard) must tolerate that.
        let saved = SavedStation {
            stationuuid: String::new(),
            name: "Local Icecast".to_string(),
            url: "http://192.168.1.10:8000/live".to_string(),
            ..Default::default()
        };
        let station = station_from_saved(&saved);
        assert_eq!(station.stream_url(), "http://192.168.1.10:8000/live");
        assert!(station.stationuuid.is_empty());
    }
}
