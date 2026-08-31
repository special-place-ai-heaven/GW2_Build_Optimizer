//! Codec-support characterization tests for the shipping decode path.
//!
//! Every test drives the EXACT construction `player::open_and_append` uses —
//! `rodio::Decoder::builder().with_data(reader).with_seekable(false)` plus
//! the mime hint a live station's Content-Type would supply — against tiny
//! local fixtures. No sockets, no audio device: samples are pulled straight
//! off the `Source` iterator.
//!
//! ## Fixture provenance (`testdata/`, regenerable offline)
//!
//! - `tone_aac_lc.aac` — 2 s 440 Hz mono sine, AAC-LC in ADTS:
//!   `ffmpeg -f lavfi -i sine=frequency=440:sample_rate=44100:duration=2
//!    -ac 1 -c:a aac -b:a 24k -f adts tone_aac_lc.aac`
//! - `tone.mp3` — same tone via `-c:a libmp3lame -b:a 32k -f mp3`.
//! - `tone_aac_he_sim.aac` — `tone_aac_lc.aac` post-processed by
//!   `testdata/make_he_sim.py`: eight spec-valid FIL elements (id_syn_ele 6)
//!   with EXT_SBR_DATA-typed payloads spliced into every frame's raw data
//!   block and the 13-bit ADTS frame_length bumped to match. This simulates
//!   the on-wire shape of HE-AAC ("AAC+"): implicit ADTS signaling (header
//!   says LC) with the SBR payload riding in FIL elements.
//!
//! ## Why the simulation is conclusive for the AAC+ verdict
//!
//! No HE-AAC encoder exists on the build machine (ffmpeg native `aac` is
//! LC-only, `aac_mf` rejects `-profile:a`, no libfdk), so a true HE-AAC
//! fixture could not be generated offline. But symphonia-codec-aac 0.5.5
//! (`aac/mod.rs`, `decode_ga`, arm `6 => // ID_FIL`) skips FIL payloads
//! content-blind — `ignore_bits(count * 8)` without ever parsing the
//! extension payload — so decode behavior on real SBR data and on SBR-typed
//! filler is the same code path. What the splice can NOT prove is anything
//! about SBR reconstruction; there is none: a real AAC+ station decodes as
//! its LC core — correct pitch and tempo, but missing the SBR high band
//! (and HE-AAC v2 parametric stereo folds to the mono core). Degraded yet
//! audible, which is why `directory::codec_supported` keeps AAC+/AACP.

use std::io::Cursor;

use rodio::decoder::DecoderError;
use rodio::{Decoder, Source};

const AAC_LC: &[u8] = include_bytes!("testdata/tone_aac_lc.aac");
const AAC_HE_SIM: &[u8] = include_bytes!("testdata/tone_aac_he_sim.aac");
const MP3: &[u8] = include_bytes!("testdata/tone.mp3");

/// FIL bytes `make_he_sim.py` splices into each of the 88 fixture frames.
const FIL_PREFIX_LEN: usize = 53;
const FIXTURE_FRAMES: usize = 88;

/// Mirror of `player::open_and_append`'s decoder construction: unseekable
/// data plus the mime hint the station's Content-Type header would prime.
fn shipping_decode(
    bytes: &'static [u8],
    mime: Option<&str>,
) -> Result<Decoder<Cursor<&'static [u8]>>, DecoderError> {
    let mut builder = Decoder::builder()
        .with_data(Cursor::new(bytes))
        .with_seekable(false);
    if let Some(mime) = mime {
        builder = builder.with_mime_type(mime);
    }
    builder.build()
}

/// Drain a decoder into (sample_rate, channels, samples) off the `Source`
/// iterator — the same samples the sink (and the visualizer tap) would pull.
fn pcm(decoder: Decoder<Cursor<&'static [u8]>>) -> (u32, u16, Vec<f32>) {
    let rate = decoder.sample_rate().get();
    let channels = decoder.channels().get();
    let samples: Vec<f32> = decoder.collect();
    (rate, channels, samples)
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
}

#[test]
fn aac_lc_adts_decodes_on_the_shipping_path() {
    let decoder = shipping_decode(AAC_LC, Some("audio/aac"))
        .expect("AAC-LC over ADTS must decode — red alert if this fails");
    let (rate, channels, samples) = pcm(decoder);
    assert_eq!(rate, 44_100);
    assert_eq!(channels, 1);
    // 88 ADTS frames x 1024 samples; a shortfall means frames were dropped.
    assert_eq!(samples.len(), FIXTURE_FRAMES * 1024);
    assert!(
        peak(&samples) > 0.1,
        "the tone must be audible, not digital silence"
    );
}

#[test]
fn aac_plus_shaped_stream_decodes_its_lc_core_and_skips_sbr_fills() {
    // Pin the fixture shape first: the SBR-style splice is really in there.
    assert_eq!(
        AAC_HE_SIM.len(),
        AAC_LC.len() + FIXTURE_FRAMES * FIL_PREFIX_LEN,
        "he_sim fixture must be the LC fixture plus 53 FIL bytes per frame"
    );

    // `audio/aacp` is the Content-Type AAC+ icecast mounts actually send.
    let decoder = shipping_decode(AAC_HE_SIM, Some("audio/aacp"))
        .expect("HE-AAC-shaped ADTS must still open as its LC core");
    let (rate, channels, samples) = pcm(decoder);
    assert_eq!(rate, 44_100, "implicit signaling: the core rate is decoded");
    assert_eq!(channels, 1);

    // The strongest possible claim: with every FIL skipped unread, the PCM
    // is bit-identical to the plain LC fixture — the SBR payloads changed
    // nothing and broke nothing.
    let (_, _, lc_samples) = pcm(shipping_decode(AAC_LC, Some("audio/aac")).unwrap());
    assert_eq!(
        samples, lc_samples,
        "SBR fill elements must be skipped without disturbing the LC core"
    );
}

#[test]
fn corrupting_a_fil_length_derails_the_decode_proving_the_fills_are_walked() {
    // Negative control for the test above: byte 7 of the he_sim fixture is
    // the first FIL's id+count bits plus the leading esc-count bit. Flipping
    // that esc bit (0xDE -> 0xDF) inflates the declared payload from 31 to
    // 159 bytes, so the skip swallows the SCE. If the decoder were not
    // actually parsing the spliced FILs, this flip could not matter.
    let mut corrupted = AAC_HE_SIM.to_vec();
    assert_eq!(corrupted[7], 0xDE, "fixture layout changed under the test");
    corrupted[7] = 0xDF;
    let leaked: &'static [u8] = corrupted.leak();
    let clean_len = FIXTURE_FRAMES * 1024;
    // An outright build refusal would prove the same sensitivity; what must
    // never happen is a full clean decode of the corrupted stream.
    if let Ok(decoder) = shipping_decode(leaked, Some("audio/aacp")) {
        let (_, _, samples) = pcm(decoder);
        assert_ne!(
            samples.len(),
            clean_len,
            "a wrong FIL length must not decode to a full clean stream"
        );
    }
}

#[test]
fn adts_probe_survives_aacp_and_missing_mime_hints() {
    // symphonia's AdtsReader only declares `audio/aac`; `audio/aacp` and a
    // missing Content-Type must fall through to the 0xFFF1 content marker.
    for mime in [Some("audio/aacp"), None] {
        let decoder = shipping_decode(AAC_LC, mime)
            .unwrap_or_else(|e| panic!("ADTS probe failed with mime {mime:?}: {e}"));
        let (rate, _, samples) = pcm(decoder);
        assert_eq!(rate, 44_100, "mime {mime:?}");
        assert_eq!(samples.len(), FIXTURE_FRAMES * 1024, "mime {mime:?}");
    }
}

#[test]
fn mp3_control_decodes_on_the_shipping_path() {
    let decoder = shipping_decode(MP3, Some("audio/mpeg")).expect("MP3 control must decode");
    let (rate, channels, samples) = pcm(decoder);
    assert_eq!(rate, 44_100);
    assert_eq!(channels, 1);
    // ~2 s of 44.1 kHz mono, give or take lame's encoder delay/padding.
    assert!(
        (80_000..=100_000).contains(&samples.len()),
        "unexpected MP3 sample count: {}",
        samples.len()
    );
    assert!(peak(&samples) > 0.1);
}
