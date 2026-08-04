#!/usr/bin/env python3
"""One-shot, fail-closed scenario alignment for PR #44."""

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
OLD = """    vi.spyOn(window, 'prompt').mockReturnValueOnce('Дата подтверждена по оригиналу');
    fireEvent.click(within(exceptionCard as HTMLElement).getByRole('button', { name: 'Закрыть' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'resolve_automation_exception')).toBe(true));
    vi.spyOn(window, 'confirm').mockReturnValueOnce(true);
    fireEvent.click(within(exceptionCard as HTMLElement).getByRole('button', { name: /Подтвердить всё/ }));
    await waitFor(() => expect(calls.some((c) => c.command === 'confirm_risk_exception_and_retry')).toBe(true));
"""
NEW = """    fireEvent.click(within(exceptionCard as HTMLElement).getByRole('button', { name: 'Закрыть' }));
    const resolveDialog = await screen.findByRole('dialog', { name: 'Закрыть исключение' });
    fireEvent.change(within(resolveDialog).getByLabelText('Что исправлено или подтверждено? *'), { target: { value: 'Дата подтверждена по оригиналу' } });
    fireEvent.click(within(resolveDialog).getByRole('button', { name: 'Закрыть исключение' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'resolve_automation_exception')).toBe(true));
    const refreshedExceptionCard = (await screen.findByText(/Нужно подтвердить дату/)).closest('.exceptionItem');
    expect(refreshedExceptionCard).toBeTruthy();
    fireEvent.click(within(refreshedExceptionCard as HTMLElement).getByRole('button', { name: /Подтвердить всё/ }));
    const riskDialog = await screen.findByRole('dialog', { name: 'Подтвердить спорные значения?' });
    fireEvent.click(within(riskDialog).getByRole('button', { name: 'Подтвердить и повторить' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'confirm_risk_exception_and_retry')).toBe(true));
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
        raise RuntimeError(f"expected one legacy exception-dialog block, found {source.count(OLD)}")
    updated = source.replace(OLD, NEW, 1)
    if OLD in updated or updated.count(NEW) != 1:
        raise RuntimeError("scenario replacement postcondition failed")
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
    run("git", "commit", "-m", "test(ui): use native dialogs for exception scenarios")
    run("git", "push", "origin", f"HEAD:{branch}")

    return run(sys.executable, SCRIPT, *sys.argv[1:], check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
