#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
test_path = ROOT / "src/App.scenarios.test.tsx"
verify_path = ROOT / "scripts/verify_source_manifest.py"
original_verify_path = ROOT / "verification/original_verify_source.py"
self_path = Path(__file__)
error_path = ROOT / "verification/scenario_test_error.txt"

payload = test_path.read_text(encoding="utf-8")
old = """    fireEvent.click(screen.getByRole('button', { name: 'Снять выбор' }));
    fireEvent.click(screen.getByRole('checkbox', { name: 'Добавить Счёт на оплату в комплект' }));
    await click(/Создать документы \\(1\\)/);"""
new = """    fireEvent.click(screen.getByRole('button', { name: 'Снять выбор' }));
    const invoiceTile = screen.getByRole('button', { name: 'Счёт на оплату' });
    expect(invoiceTile.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(invoiceTile);
    await waitFor(() => expect(invoiceTile.getAttribute('aria-pressed')).toBe('true'));
    await click(/Создать документы \\(1\\)/);"""
if payload.count(old) != 1:
    raise RuntimeError(f"expected one obsolete checkbox scenario, found {payload.count(old)}")
test_path.write_text(payload.replace(old, new, 1), encoding="utf-8")

subprocess.run(["npm", "ci"], cwd=ROOT, check=True)
subprocess.run(["npm", "run", "typecheck"], cwd=ROOT, check=True)
result = subprocess.run(["npm", "test"], cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
if result.returncode != 0:
    error_path.write_text(result.stdout, encoding="utf-8")
    subprocess.run(["git", "config", "user.name", "github-actions[bot]"], cwd=ROOT, check=True)
    subprocess.run(["git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"], cwd=ROOT, check=True)
    subprocess.run(["git", "add", "verification/scenario_test_error.txt"], cwd=ROOT, check=True)
    subprocess.run(["git", "commit", "-m", "Capture full scenario repair test output"], cwd=ROOT, check=True)
    subprocess.run(["git", "push", "origin", "HEAD:agent/fix-simple-button-creation"], cwd=ROOT, check=True)
    raise SystemExit(result.returncode)

original_verify = original_verify_path.read_text(encoding="utf-8")
verify_path.write_text(original_verify, encoding="utf-8")
original_verify_path.unlink(missing_ok=True)
self_path.unlink(missing_ok=True)
error_path.unlink(missing_ok=True)

build_path = ROOT / "scripts/build_source_archive.py"
spec = importlib.util.spec_from_file_location("build_source_archive", build_path)
if spec is None or spec.loader is None:
    raise RuntimeError("cannot load source archive module")
source_archive = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source_archive)
(ROOT / source_archive.SOURCE_MANIFEST).write_bytes(source_archive.source_manifest_payload())

subprocess.run(["python", "tests/test_v18_0_3_regression_contracts.py"], cwd=ROOT, check=True)
subprocess.run(["git", "config", "user.name", "github-actions[bot]"], cwd=ROOT, check=True)
subprocess.run(["git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"], cwd=ROOT, check=True)
subprocess.run(["git", "add", "-A"], cwd=ROOT, check=True)
subprocess.run(["git", "commit", "-m", "Align full scenario test with whole-tile selection"], cwd=ROOT, check=True)
subprocess.run(["git", "push", "origin", "HEAD:agent/fix-simple-button-creation"], cwd=ROOT, check=True)
