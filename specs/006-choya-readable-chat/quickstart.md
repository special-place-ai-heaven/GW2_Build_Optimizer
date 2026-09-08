# Quickstart: Choya readable chat

Unit checks run on the workspace; the rest is in-game with the DLL, because the bubble, the tabs and the stream are things a player sees.

## Unit checks

```bash
cargo test -p gw2-build-optimizer --lib -- chat_markup tab_kind provider_picks request_kind live_output
cargo test -p gw2-optimizer --lib -- llm::live prompts::reply_shape
cargo test --workspace   # locale parity (#21) included
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: every new test green; the Sprint 2 experiments and `pve_output_unchanged_by_conditional_tagging` untouched and green.

## In-game (after `cargo build --release` and copying the DLL to the addons folder)

1. **Whole, readable reply.** Ask Choya for a build with a rotation. Expected: bullets on their own lines, names in the accent colour and bold, the rotation as `A → B → C`, any address underlined; nothing ends in three dots; a long reply scrolls inside the bubble.
2. **Tabs.** Run Optimize, then open a GuildJen card. Expected: a Current tab (blue tint), an Optimized tab (green tint), a `GuildJen · <build>` tab (site colour); clicking Optimized brings the optimizer's build back. Load a saved build: it is added, nothing disappears.
3. **Cards persist.** After a plated build with cards, send "score that exact build". Expected: cards still there after the answer.
4. **Fallback matches the request.** Pick a model that will not answer (a wrong key on a spare provider), ask "score that exact build". Expected: bullets with viable, gates, score and coverage, no optimizer build. Ask "make me a reaper build". Expected: the optimizer's Reaper build, not the equipped profession's.
5. **Thinking bubble.** On a free model, send a build request. Expected: the bubble names the step and counts seconds; clicking expands to live reasoning or the streaming answer; if nothing arrives, "nothing for N s" appears within 20 s. Press Stop: the bubble ends within a second and the partial text stays as a reply with a Retry button. Press Retry: Choya continues rather than restarting.
6. **Log.** Open `addons/Nexus/Nexus.log`. Expected: one `Choya <step>: … in Ns` line per step of that request.

Report each as pass or fail with the text seen. The DLL stays local until the Sprint 1 release condition holds.
