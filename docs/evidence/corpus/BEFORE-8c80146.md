# Corpus BEFORE — tip `8c80146ed71c75f437a5df8aa4d1c296c91d5735` (v1.14.5)

Recorded: 2026-09-13T11:52:29Z (Europe/Ljubljana local run on Fresh)
Machine: Fresh `C:\AI_STUFF\PROGRAMMING\GW2_Build_Optimizer`
Addon cache: `C:\GAMES\Guild Wars 2\addons\gw2_build_optimizer`

## Verdict

**BEFORE Success [1] = DOCUMENTED GAP + best-available calibration evidence.**

Meta≥Average pairwise ordering and Spearman ρ were **NOT checked**.
Expert labels Meta/Great/Good/Average (ledger text: 26 Meta + 24 Great + 53 Good + 25 Average = **128**, often rounded as "~130") are **not present** in scraped `BenchmarkBuild` JSON and were **not found on disk**. Per Grove: do not invent labels.

Scorer untouched.

## Harness used (existing — no new scorer)

| Tool | Command | Role |
|------|---------|------|
| calibrate_viability | `cargo run -p gw2-optimizer --example calibrate_viability` | Gate pass rates on synced published builds via `evaluate_validated_build_with` |
| flow_calibration | `cargo run --release -p gw2-optimizer --example flow_calibration [PvE\|WvW\|PvP]` | Realized-axis norms on synergy seeds (not published-corpus ranking) |

Paths: `crates/optimizer/examples/calibrate_viability.rs`, `crates/optimizer/examples/flow_calibration.rs`

Raw stdout:
- `docs/evidence/corpus/calibrate_viability-8c80146.txt`
- `docs/evidence/corpus/flow_calibration-pve-8c80146.txt`
- `docs/evidence/corpus/flow_calibration-wvw-8c80146.txt`
- `docs/evidence/corpus/flow_calibration-pvp-8c80146.txt`

## Labels gap (why Spearman / Meta≥Average absent)

- `BenchmarkBuild` fields: source, profession, mode, role, gear_prefix, published ids — **no** expert_label / tier / Meta|Great|Good|Average.
- Live scrape on this machine: **740** synced builds total (`calibrate_viability` stdout: "740 synced builds"). Per-source split not emitted by that harness and not claimed here.
- Ledger oracle text remains aspirational until a labels fixture exists (`data/corpus/labels.json` or equivalent join key).

## calibrate_viability (published corpus — best available)

- Synced builds: **740**; skills in db: 4702; palette: 660
- Scored: **582**; with published opener: 205; validation rejects: 158; no-plate skips: 0
- `is_viable` (every gate): **489/582 (84%)**
- Blocking-gates-only viable: **489/582 (84%)**

Gate pass% (all modes mixed): CleanseRate 80%, EffectiveHealth 96%, EncounterOutcome 32%, HarasserStrip 11%, MobilityOut 100%, ProtectedExecution 30%, ResourceLegality 77%, SecureCompletion 64%, StabilityAccess 96%, StunbreakCount 88%, SustainRecovery 97%.

Worst whole-build roles (0% pass samples): Cloud DPS, Cloud Tank, DPS, Damage, Duelist, Havoc Assassin/DPS, Power DPS, Roamer, Roaming Assassin/Bruiser/Condi DPS, Sidenoder, Zerg Boon DPS (among others in the full log).

**Not checked by this tool:** Meta≥Average pairwise, Spearman vs expert tiers, cross-mode label correlation, rank stability under Phase 2+ scorer moves.

## flow_calibration (synergy seeds — axis norms, not Meta ordering)

Ran PvE + WvW + PvP (36 seeds each: 9 professions × power/condi/healer/disrupt).

PvE highlights (strike / condi / hps for power|condi|healer presets — see raw logs for full table):
- Warrior power strike ~45168; Engineer condi ~11639; Necro power ~38327; Revenant power ~5168 (low vs others).
- Condition skew still visible (Engineer condi high vs several professions) — qualitative BEFORE for Success [8], not a Meta-vs-Average metric.

**Not checked by this tool:** published-build ordering, Meta/Great/Good/Average, Spearman.

## Gaps / blockers for a true Success [1] metric

1. **Missing labels fixture** for the ledger's expert-tier builds (128 from stated bucket counts; "~130" in prose).
2. No durable Meta≥Average / Spearman harness joined to labels (would wrap existing referee score only — not started, to avoid inventing tiers).
3. `score_benchmark_build` remains a gear-prefix proxy — unsuitable for corpus calibration.

## Next (out of PHASE 0 scope)

When labels land: join on `source_url`/`build_code`, score with `user_intent_score` from `evaluate_validated_build_with`, emit Meta≥Average + Spearman per mode; no scorer change.
