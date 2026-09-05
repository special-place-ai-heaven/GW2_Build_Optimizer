# Remediation contracts

User override (2026-09-05): work in small sprints and commit often. Verified local sprint commits may precede in-game acceptance, superseding wait-before-commit wording below. Stage only owned changes; another agent is actively modifying this shared tree. No push/release is implied.
## Persistence
NotFound is first run; all other read/parse errors are observable and inhibit destructive replacement. Interrupted Sending messages become Failed in memory only after successful deserialization. Existing feedback/taxonomy overwrite uses storage::replace_file. Failed staging/publication leaves the previous file intact.

## Rendering
Malformed specialization lists must not index outside BuildLocks::specs. No blocking DNS/network work is introduced on the render thread. Per-slot stat presentation uses validated gear.

## Address restrictions
Normalize bracketed IPv6 once; compare literal IPs against the shared reserved-address policy. Check DNS on workers, including redirect targets. An unresolved public hostname may fail through the normal transport; it must not turn a reserved literal into an allowed URL.

## Verification and closure
Behavioral failures need regression coverage. Refactoring needs relevant consumer tests plus strict compilation/Clippy. Report excerpts alone never close a finding. Do not weaken CI or replace failing behavior tests with source-string checks.

## Release
Build and test the Windows addon locally. Read addons_dir from ignored dev.cfg without exposing unrelated values; preserve a recoverable previous DLL before replacement. User in-game acceptance is recorded per batch before commit/push/release. Server Docker operations remain VPS-only and are not part of this local handoff.
