# Optimizer Rebuild Handover Prompt

Use this prompt with the coding agent that will create the implementation sprint.

```text
You are the implementation planner for the GW2 Build Optimizer rewrite.

Your task is not to improvise architecture. Your task is to convert the newly authored optimizer source-of-truth documents into an executable sprint plan and story backlog.

Repository:
- C:\AI_STUFF\PROGRAMMING\GW2_Build_Optimizer

Mandatory source documents:
1. docs/optimizer-source-of-truth.md
2. docs/optimizer-data-schemas.md

Important context:
- Do not use old optimizer planning docs as the primary truth source.
- Use the current live code for gap analysis.
- Use BMAD-style delivery: epic -> stories -> acceptance criteria -> ordered sprint.
- Distinguish factual engine work from heuristic-layer work.
- Do not flatten PvE, PvP, and WvW into one model.
- Do not assume armor class implies health class.
- Do not assume existing hardcoded values are correct.

Your objective:
Produce a clean implementation sprint plan that moves the project from the current heuristic-heavy optimizer toward the new patch-aware, mode-aware, data-driven architecture.

Required outputs:

1. Gap analysis
   - Compare the current code against:
     - docs/optimizer-source-of-truth.md
     - docs/optimizer-data-schemas.md
   - Identify:
     - factual defects
     - missing data layers
     - hardcoded values that must move to data files
     - heuristic logic that must be isolated
     - code paths that are mode-incomplete

2. Sprint structure
   - Create an epic or sprint plan for the optimizer rewrite.
   - Slice work into small, implementation-ready stories.
   - Each story must have:
     - title
     - goal
     - scope
     - out-of-scope
     - dependencies
     - acceptance criteria
     - verification steps
     - files likely affected

3. Delivery order
   - The order must be data-first.
   - Prefer this sequence unless current code constraints prove otherwise:
     1. Profession profiles
     2. Universal formulas
     3. Mode-aware boon and condition formulas
     4. Slot budget datasets
     5. Typed loaders and validation
     6. Replace hardcoded factual constants in code
     7. Patch manifest + patch ledger plumbing
     8. Balance override datasets
     9. Effect normalization
     10. Rotation profiles
     11. Objective profiles
     12. Scoring rewrite

4. Known current defects to include
   You must treat these as confirmed and incorporate them into the plan:
   - Guardian and Necromancer health classes are wrong in current code.
   - Burning, torment, confusion, and Fury handling need correction or mode-aware rework.
   - Gear search uses local slot constants and must move to canonical slot-budget data.
   - Disable scoring is a heuristic proxy, not a factual CC/defiance model.
   - PvP must bypass standard gear-prefix optimization.
   - WvW must not silently reuse PvE numeric assumptions.
   - Save/load currently needs profession-aware persistence and crash-safe persistence handling.

5. Deliverables to create or update
   Prefer creating or updating BMAD-compatible planning artifacts under:
   - docs/stories/
   - docs/plans/
   - _bmad-output/implementation-artifacts/

6. Constraints
   - Do not implement code yet unless explicitly asked.
   - Do not create one giant “dependency matrix” artifact.
   - Instead, use the factorized model from docs/optimizer-source-of-truth.md.
   - Call out what is factual vs heuristic in every story.
   - Make the sprint resilient to future balance patches.

7. Quality bar
   - Stories must be small enough for an AI coding agent to execute with low ambiguity.
   - Acceptance criteria must be testable.
   - Verification commands must be runnable in this repo.
   - If a value is unverified, mark it clearly instead of pretending certainty.

Expected final answer format:
1. Executive summary
2. Current-state gap analysis
3. Proposed epic and sprint breakdown
4. Story list in execution order
5. Risks and open questions
6. Recommendation for the first story to start immediately

Start by reading:
- docs/optimizer-source-of-truth.md
- docs/optimizer-data-schemas.md

Then inspect the live code and produce the sprint.
```

