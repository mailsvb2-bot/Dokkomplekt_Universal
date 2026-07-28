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
    '<button className="primaryBtn" onClick={props.onRunZeroTouch} disabled={props.busy}>Создать комплект</button>',
    '<button className="primaryBtn" onClick={props.onRunZeroTouch} disabled={props.busy}>Обработать указанный файл</button>',
)
replace_once(
    'src/App.scenarios.test.tsx',
    '    await click(/Создать документы \\(1\\)/);',
    "    await click(/^Создать комплект$/);",
)
text_path = ROOT / 'src/App.scenarios.test.tsx'
text = text_path.read_text('utf-8')
old = 'screen.getByText(/Перетащите документ в эту область/)'
count = text.count(old)
if count != 2:
    raise RuntimeError(f'expected 2 old drop-zone labels, found {count}')
text_path.write_text(text.replace(old, 'screen.getByText(/Перетащите сюда исходный документ/)'), 'utf-8')

Path(__file__).unlink()
module_path = ROOT / 'scripts' / 'build_source_archive.py'
spec = importlib.util.spec_from_file_location('build_source_archive', module_path)
if spec is None or spec.loader is None:
    raise RuntimeError('cannot load build_source_archive')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
(ROOT / module.SOURCE_MANIFEST).write_bytes(module.source_manifest_payload())
print('ONE-CLICK UI TEST FIXES APPLIED')
