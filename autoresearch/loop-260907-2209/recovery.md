# Resume evidence, 2026-09-08

The first strict six-attempt baseline process exited 1 after 566.970 seconds.
Terminal Commander reported its receipt as reconstructed after a daemon restart;
the bucket and raw frame history were unavailable. Do not infer its missing four
results or treat the command as successful.

Results observed before restart: Gemini PASS 74.8 seconds / 7 wire; Sante FAIL
31.9 seconds / 4 wire (two specializations). These failures remain part of the
evidence and are not overwritten by fresh measurements.

The original strict-baseline executable was last written 2026-09-07 22:12:28
local time, before the prompt edit. A local ignored copy in this directory runs
the fresh durable baseline. `run.cjs` records only numeric results and tool names
to JSONL files as they arrive; it does not retain provider error bodies.

Candidate prompt commit: 3ac6d64. Initial candidate guard found two old prompt
snapshots; after updating their literal expectations, the full guard passed:
1112 passed, 0 failed, 4 ignored, 16.42 seconds test duration. Formatting passed.
Strict Clippy (`cargo clippy -p gw2-optimizer --lib --examples -- -D warnings`)
and the candidate example build also passed.

Configured-provider availability was inspected by presence only: Gemini and
OpenRouter are configured, OpenAI and Anthropic are not. No credential values
were printed or written to the experiment. Native Gemini smoke is attempted
separately from the OpenRouter latency comparison.

The pinned predicate remains unchanged. A failed baseline or holdout keeps it
PENDING; do not label the study converged merely because candidate timings fall.

Second resume, 2026-09-08 17:17 local: Terminal Commander returned JobLost for
the candidate-2 batch and candidate-3 guard started before the pause. No
choya_live or cargo process remained. Candidate-2 JSONL contains four completed
attempts and a partial third Gemini attempt; there is no exit record. Keep the
partial attempt as interrupted, not PASS/FAIL. The current guard was rerun with
durable capture: 1112 passed, 4 ignored, exit 0 (`guard-3.jsonl`).

Candidate 3 combines the prefix/completeness correction from 2d11cb6 with the
direct-plate experiment reintroduced by fb0136f, then requires all three reference
sections via efd83fe. This is a new hypothesis after the missing-prefix diagnostic,
not retention of the failed first candidate. Timing comparisons across the pause
are exploratory because provider conditions can change.
