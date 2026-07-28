from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text('utf-8')
    if old not in text:
        raise RuntimeError(f'pattern not found in {path}: {old!r}')
    target.write_text(text.replace(old, new, 1), 'utf-8')


replace_once(
    'src/components/Workspace.tsx',
    '          <label className="checkLine"><input type="checkbox" checked={props.autoPrint} onChange={(event) => props.setAutoPrint(event.target.checked)} /><span>Печатать готовый комплект автоматически</span></label>\n          {props.intakeResult &&',
    '          <label className="checkLine"><input type="checkbox" checked={props.autoPrint} onChange={(event) => props.setAutoPrint(event.target.checked)} /><span>Печатать готовый комплект автоматически</span></label>\n          <small className="automationHelp">Если файл временно нельзя прочитать, рядом появится заметка «НЕ ПРОЧИТАН.txt» с понятной причиной и временем следующей попытки.</small>\n          {props.intakeResult &&',
)

replace_once(
    'tests/test_v18_0_3_regression_contracts.py',
    '        self.assertGreaterEqual(app.count("setSelectedDocIds([])"), 3)\n        self.assertIn("aria-pressed={selected}", rail)',
    '        self.assertIn("setSelectedDocIds(res.pack.documents.map((document) => document.id))", app)\n        self.assertIn("setSelectedDocIds(pack.documents.map((document) => document.id))", app)\n        self.assertIn("onGenerateSelected={generateSelectedDocuments}", app)\n        self.assertIn("aria-pressed={selected}", rail)',
)

replace_once(
    'tests/test_v18_3_0_hardening_contracts.py',
    '        self.assertEqual(len(backend), 113)',
    '        self.assertEqual(len(backend), 114)',
)

Path(__file__).unlink()
module_path = ROOT / 'scripts' / 'build_source_archive.py'
spec = importlib.util.spec_from_file_location('build_source_archive', module_path)
if spec is None or spec.loader is None:
    raise RuntimeError('cannot load build_source_archive')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
(ROOT / module.SOURCE_MANIFEST).write_bytes(module.source_manifest_payload())
print('ONE-CLICK CONTRACTS UPDATED')
