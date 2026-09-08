"""Seen-failing harness for the simulator-trust experiments.

Usage (from the repository root):

    python docs/audit/disable_and_run.py <control> [<control> ...]
    python docs/audit/disable_and_run.py --list

Each control names one mechanism to disable, the exact anchor text to replace
in one source file, the replacement, and the test filter that must FAIL while
the mechanism is off. The script copies the file, applies the edit in memory
(the anchor must occur exactly once), writes it back with the file's own line
ending, runs the filtered test, restores the copy (never `git checkout`, the
tree holds uncommitted work between tasks) and prints the panic block and the
test result line. Paste that block under the control's heading in
`docs/audit/sprint2-failures.md` (Sprint 3: `sprint3-failures.md`).
"""
import io
import os
import re
import shutil
import subprocess
import sys

# Windows consoles default to cp1252; the trace carries arrows and multiplication signs.
sys.stdout.reconfigure(encoding="utf-8", errors="replace")

WT = "crates/optimizer/src/rotation/wvw_timeline.rs"
EN = "crates/optimizer/src/engine.rs"

# name -> (file, anchor, replacement, test filter)
CONTROLS = {
    # Sprint 1 (kept so the audit's section 6 stays reproducible)
    "negative-a": (
        EN,
        "let effects = crate::data::normalized_effects::effects().effects_for_mode(mode);",
        'let effects = crate::data::normalized_effects::effects().effects_for_mode("PvE");',
        "reaper_negative_control_wrong_mode_and_stowed_set",
    ),
    "late-buff": (
        WT,
        'let might = self.buff_stacks("Might") as f64;',
        'let might = 0.0 * self.buff_stacks("Might") as f64;',
        "reaper_timing_icd_interrupt_and_late_buff",
    ),
    # Sprint 2
    "oncrit": (
        WT,
        "                if crit > 0.0 {\n                    for _ in 0..*hit_count {",
        "                if crit > 0.0 && false {\n                    for _ in 0..*hit_count {",
        "reaper_oncrit_positive_control_fires_from_crits",
    ),
    "swap": (
        WT,
        "spec.weapon_set = sigil_sets.get(&spec.source_id).copied().unwrap_or(0);",
        "spec.weapon_set = sigil_sets.get(&spec.source_id).copied().unwrap_or(0).min(1);",
        "reaper_swap_loads_set_two_sigils",
    ),
    "threshold": (
        WT,
        "                    let holds = if above {",
        "                    let holds = true || if above {",
        "reaper_scholar_applies_only_above_threshold",
    ),
    "stack": (
        WT,
        "            spec.stacks = (spec.stacks + 1).min(max);",
        "            spec.stacks += 1;",
        "reaper_thief_stacks_cap_and_expire",
    ),
    "dark": (
        WT,
        "                let damage = 198.0 + 0.03 * self.params.power;",
        "                self.note_unmodeled(unmodeled(\"dark field\"));\n                return;\n                #[allow(unreachable_code)]\n                let damage = 198.0 + 0.03 * self.params.power;",
        "reaper_dark_whirl_life_steals",
    ),
    "shroud_floor": (
        WT,
        "        self.resources.get(&rule.kind).copied().unwrap_or(0.0) >= rule.cost.max(rule.entry_floor)",
        "        self.resources.get(&rule.kind).copied().unwrap_or(0.0) >= rule.cost",
        "reaper_shroud_refused_without_life_force",
    ),
    "drain": (
        WT,
        "            *pool = (*pool - drain * seconds).max(0.0);",
        "            *pool = (*pool - 0.0 * drain * seconds).max(0.0);",
        "reaper_shroud_drains_and_exits",
    ),
    # Sprint 3 controls (specs/007-trait-triggers): filled as each mechanism lands
    "shroud_enter": (WT, "", "", "necro_shroud_enter_fires_once_at_entry"),
    "prereq": (WT, "", "", "necro_chilled_prerequisite_gates_chilling_nova"),
    "scope": (WT, "", "", "necro_shout_scope_fires_on_shouts_only"),
    "population": (WT, "", "", "population_havoc_credits_five_or_cap"),
    "coverage": (EN, "", "", "coverage_line_never_names_executed_weapon_skills"),
}


def run(name):
    path, anchor, replacement, test = CONTROLS[name]
    if not anchor:
        print(f"### {name}\nNOT YET FILLED (mechanism not landed)")
        return 2
    backup = path + ".seen-failing.bak"
    shutil.copyfile(path, backup)
    text = io.open(path, encoding="utf-8", newline="").read()
    nl = "\r\n" if "\r\n" in text else "\n"
    old = anchor.replace("\n", nl)
    count = text.count(old)
    if count != 1:
        os.remove(backup)
        print(f"### {name}\nANCHOR MISS: {count} occurrences of {anchor[:80]!r} in {path}")
        return 2
    io.open(path, "w", encoding="utf-8", newline="").write(
        text.replace(old, replacement.replace("\n", nl))
    )
    try:
        env = dict(os.environ, MSYS_NO_PATHCONV="1")
        proc = subprocess.run(
            ["cargo", "test", "-p", "gw2-optimizer", "--lib", test, "--", "--nocapture"],
            capture_output=True, text=True, encoding="utf-8", errors="replace", env=env,
        )
        out = proc.stdout + proc.stderr
    finally:
        shutil.copyfile(backup, path)
        os.remove(backup)
    restored = io.open(path, encoding="utf-8", newline="").read()
    assert restored == text, f"{path} was not restored byte-identically"
    panics = [m.group(0).strip() for m in re.finditer(r"panicked at [^\n]*\n((?:[^\n]*\n){1,4})", out)]
    result = re.search(r"test result: [^\n]*", out)
    errors = [line for line in out.splitlines() if line.startswith("error")]
    print(f"### {name}\nfile: {path}\ndisabled: {anchor.strip()[:100]}\ntest: {test}")
    print("\n".join(panics) if panics else "NO PANIC CAPTURED")
    print(result.group(0) if result else "NO RESULT LINE")
    if errors:
        print("\n".join(errors[:6]))
    print("restored: byte-identical\n")
    return 0 if (panics or errors) else 1


def main(argv):
    if not argv or argv == ["--list"]:
        print("\n".join(CONTROLS))
        return 0
    if not os.path.exists("Cargo.toml"):
        print("run from the repository root", file=sys.stderr)
        return 2
    return max(run(name) for name in argv)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
