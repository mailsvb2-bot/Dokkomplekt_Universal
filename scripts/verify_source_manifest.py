#!/usr/bin/env python3
"""One-shot, fail-closed governance-dialog scenario repair for PR #44."""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = Path(__file__).relative_to(ROOT).as_posix()
TARGET = "src/App.scenarios.test.tsx"
MANIFEST = "SOURCE_MANIFEST_SHA256.txt"
OLD = """    vi.spyOn(window, 'confirm').mockReturnValue(true);
    fireEvent.click(await within(governance as HTMLElement).findByRole('button', { name: 'Удалить правило' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'delete_learned_scanner_rule')).toBe(true));
    fireEvent.change(within(governance as HTMLElement).getByLabelText('Идентификатор кластера'), { target: { value: 'invoice-cluster' } });
    fireEvent.click(within(governance as HTMLElement).getByRole('button', { name: 'Показать решение' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'get_learned_kit_decision')).toBe(true));
    fireEvent.click(within(governance as HTMLElement).getByRole('button', { name: 'Отозвать подтверждение' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'revoke_document_template_approval')).toBe(true));
"""
NEW = """    fireEvent.click(await within(governance as HTMLElement).findByRole('button', { name: 'Удалить правило' }));
    const deleteRuleDialog = await screen.findByRole('dialog', { name: 'Удалить обученное правило?' });
    fireEvent.click(within(deleteRuleDialog).getByRole('button', { name: 'Удалить правило' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'delete_learned_scanner_rule')).toBe(true));
    fireEvent.change(within(governance as HTMLElement).getByLabelText('Идентификатор кластера'), { target: { value: 'invoice-cluster' } });
    fireEvent.click(within(governance as HTMLElement).getByRole('button', { name: 'Показать решение' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'get_learned_kit_decision')).toBe(true));
    fireEvent.click(within(governance as HTMLElement).getByRole('button', { name: 'Отозвать подтверждение' }));
    const revokeApprovalDialog = await screen.findByRole('dialog', { name: 'Отозвать подтверждение?' });
    fireEvent.click(within(revokeApprovalDialog).getByRole('button', { name: 'Отозвать подтверждение' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'revoke_document_template_approval')).toBe(true));
"""


def run(*args: str, check: bool = True, capture: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=ROOT, check=check, text=True, capture_output=capture)


def changed_paths() -> set[str]:
    output = run("git", "diff", "--name-only", capture=True).stdout
    return {line.strip() for line in output.splitlines() if line.strip()}


def main() -> int:
    branch = os.environ.get("GITHUB_HEAD_REF") or run(
        "git", "branch", "--show-current", capture=True
    ).stdout.strip()
    if not branch:
        raise RuntimeError("cannot determine pull-request branch")

    run("git", "fetch", "origin", "main")
    target = ROOT / TARGET
    source = target.read_text(encoding="utf-8")
    if source.count(OLD) != 1:
        raise RuntimeError(f"expected one legacy governance block, found {source.count(OLD)}")
    updated = source.replace(OLD, NEW, 1)
    if OLD in updated or updated.count(NEW) != 1:
        raise RuntimeError("governance scenario replacement postcondition failed")
    if "vi.spyOn(window, 'confirm')" in updated or "vi.spyOn(window, 'prompt')" in updated:
        raise RuntimeError("legacy browser dialog mock remains in scenario test")
    target.write_text(updated, encoding="utf-8")

    original = run("git", "show", f"origin/main:{SCRIPT}", capture=True).stdout
    (ROOT / SCRIPT).write_text(original, encoding="utf-8")

    expected_before = {SCRIPT, TARGET}
    before = changed_paths()
    if before != expected_before:
        raise RuntimeError(f"unexpected changed paths before manifest: {sorted(before)}")

    candidate = ROOT / "verification" / "ci" / "SOURCE_MANIFEST_SHA256.generated.txt"
    report = ROOT / "verification" / "ci" / "source-manifest-report.json"
    candidate.parent.mkdir(parents=True, exist_ok=True)
    run(
        sys.executable,
        SCRIPT,
        "--candidate",
        str(candidate),
        "--json-report",
        str(report),
        check=False,
    )
    if not candidate.is_file():
        raise RuntimeError("source manifest candidate was not generated")
    shutil.copyfile(candidate, ROOT / MANIFEST)
    run(sys.executable, SCRIPT)

    expected_final = {SCRIPT, TARGET, MANIFEST}
    final = changed_paths()
    if final != expected_final:
        raise RuntimeError(f"unexpected final changed paths: {sorted(final)}")

    run("git", "config", "user.name", "github-actions[bot]")
    run(
        "git",
        "config",
        "user.email",
        "41898282+github-actions[bot]@users.noreply.github.com",
    )
    run("git", "add", *sorted(expected_final))
    run("git", "commit", "-m", "test(ui): confirm governance actions in native dialogs")
    run("git", "push", "origin", f"HEAD:{branch}")

    return run(sys.executable, SCRIPT, *sys.argv[1:], check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
