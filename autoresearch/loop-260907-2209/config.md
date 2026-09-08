# Choya latency experiment

Goal: reduce time to a valid model-produced plate without reducing validity.
Scope: prompt instructions first; provider-independent behavior retained; no new dependencies.
Archetype: optimize-metric. Terminal: stop-at-verified.
Budget: at most 25 focused experiments, stop on unavailable measurement or plateau.
Baseline commit: 1612f96. Initial worktree: clean.

Verify: `cargo run -p gw2-optimizer --example choya_live -- openrouter google/gemini-3.8-flash inclusionai/ling-3.0-flash-sante:free`
Guard: `cargo test -p gw2-optimizer --lib`
Acceptance: both models PASS; each model's median elapsed seconds across three baseline/candidate runs decreases without reducing pass count. Fresh holdout requests must validate before convergence. Cold handshake overhead is included by the existing harness and must be reported separately from inferred generation savings. No fallback counts as model success.

Both commands passed the installed orchestrator screen-cmd gate before execution.
Baseline guard: 1112 passed, 0 failed, 4 ignored (16.208 seconds process duration).

Hypothesis 1: unconditional simulation and synergy-tool instructions contradict prefilled evidence and cause unnecessary model turns. Make extra verification conditional on missing evidence, preserving grounded names and honest numerical claims.

Inspection: the shared loop already returns a text-only answer during Explore without a Closing call. The live harness omits the addon's deterministic reference and forces a cold handshake each run; its PASS gate checks three surviving specializations, not full slot correctness or referee acceptance. Do not generalize that gate to complete in-game correctness.
