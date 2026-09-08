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
`docs/audit/sprint2-failures.md`.
"""
import io
import os
import re
import shutil
import subprocess
import sys

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
}


def run(name):
    path, anchor, replacement, test = CONTROLS[name]
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
