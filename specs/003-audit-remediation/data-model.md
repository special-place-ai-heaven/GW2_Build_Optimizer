# Remediation state model
## Finding
Stable ID (W001–W267, B001; R001–R003 excluded), original severity/verdict, source location and claim, proposed fix, execution story/priority, dependency IDs, current decision, implementation state and verification evidence.
States: planned → investigating → implemented → verified → accepted-in-game (when applicable). Separate terminal dispositions: refuted-current, duplicate-of (only closed when canonical fix is verified), retained-deliberate (reason and trigger required). Blocked entries stay open.

## Feedback persistence
Load outcome distinguishes absent (writable empty), loaded (writable actual contents), failed (error + writes refused for this session). Refusal survives the temporary FeedbackStore object's lifetime via addon feedback state and must reach deferred writers. Normal writes use the shared Windows-safe replacement routine. No serialized schema change is necessary for this runtime guard.

## Build suggestion
Validated per-slot gear is the authority. Display stat block and combat metrics derive from the same validated kit/balance context used for ranking. Narrative is explanatory, not authoritative. Missing stats remain unknown and cannot be replaced with fabricated values.

## Ledger/task relationship
One task per actionable ID, even when duplicate IDs share a canonical implementation. A checked task means its own closure criterion and dependency are satisfied; task generation never marks a remedy complete.
