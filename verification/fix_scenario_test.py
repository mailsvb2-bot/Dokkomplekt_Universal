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


def replace_once(old: str, new: str, label: str) -> None:
    global payload
    count = payload.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one occurrence, found {count}")
    payload = payload.replace(old, new, 1)


replace_once(
    """    fireEvent.click(screen.getByRole('button', { name: 'Снять выбор' }));
    fireEvent.click(screen.getByRole('checkbox', { name: 'Добавить Счёт на оплату в комплект' }));
    await click(/Создать документы \\(1\\)/);""",
    """    fireEvent.click(screen.getByRole('button', { name: 'Снять выбор' }));
    const invoiceTile = screen.getByRole('button', { name: 'Счёт на оплату' });
    expect(invoiceTile.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(invoiceTile);
    await waitFor(() => expect(invoiceTile.getAttribute('aria-pressed')).toBe('true'));
    await click(/Создать документы \\(1\\)/);""",
    "whole-tile selection scenario",
)

replace_once(
    """      case 'import_template_file':
        return { template_path: '/app-data/user-templates/tpl.docx', extracted_text: 'Договор\\n{{org.inn}}' } as never;""",
    """      case 'import_template_file': { const req=(payload as {req?:{file_name?:string}})?.req; const fileName=req?.file_name ?? 'Договор.docx'; const title=fileName.replace(/\\.(docx|docm)$/i, ''); return { template_path: `/app-data/user-templates/${fileName}`, extracted_text: `${title}\\n{{org.inn}}` } as never; }""",
    "distinct imported template mock",
)

replace_once(
    """      case 'prepare_template_setup':
        return [{ document_id: 'tpl', template_path: 't.docx', detected_title: 'Договор', suggested_button_label: 'Договор', editable_button_label: 'Договор', role_id: 'generic', is_static_copy: false, analysis: {}, popup_fields: [] }] as never;
      case 'confirm_template_setup':
        return { pack_id: 'default', name: 'Пакет', documents: [{ ...accDoc, id: 'tpl', button_label: 'Договор' }] } as never;""",
    """      case 'prepare_template_setup': { const candidates=(payload as {req?:{candidates?:Array<{template_path:string}>}})?.req?.candidates ?? []; return candidates.map((candidate,index)=>{ const fileName=candidate.template_path.split('/').pop() ?? `Шаблон-${index+1}.docx`; const title=fileName.replace(/\\.(docx|docm)$/i, ''); return { document_id: `tpl-${index+1}`, template_path: candidate.template_path, detected_title: title, suggested_button_label: title, editable_button_label: title, role_id: 'generic', is_static_copy: false, analysis: {}, popup_fields: [] }; }) as never; }
      case 'confirm_template_setup': { const rows=(payload as {req?:{rows?:Array<{document_id:string;editable_button_label:string;template_path:string}>}})?.req?.rows ?? []; return { pack_id: 'default', name: 'Пакет', documents: rows.map((row)=>({ ...accDoc, id: row.document_id, button_label: row.editable_button_label, template_path: row.template_path })) } as never; }""",
    "distinct prepared template mock",
)

replace_once(
    """    await waitFor(() => expect(calls.some((c) => c.command === 'analyze_template_file')).toBe(true));""",
    """    await waitFor(() => expect(calls.filter((c) => c.command === 'analyze_template_file')).toHaveLength(2));""",
    "wait for both template analyses",
)

replace_once(
    """    expect(calls.filter((call) => call.command === 'analyze_template_file').some((call) => JSON.stringify(call.payload).includes('/app-data/user-templates/tpl.docx'))).toBe(true);""",
    """    expect(calls.filter((call) => call.command === 'analyze_template_file')).toHaveLength(2);""",
    "two analyzed template calls",
)

replace_once(
    """    expect(parsePayload(calls, 'prepare_template_setup')).toMatchObject({ req: { candidates: [
      { template_path: '/app-data/user-templates/tpl.docx' },
      { template_path: '/app-data/user-templates/tpl.docx' },
    ] } });""",
    """    expect(parsePayload(calls, 'prepare_template_setup')).toMatchObject({ req: { candidates: [
      { template_path: '/app-data/user-templates/Договор.docx' },
      { template_path: '/app-data/user-templates/Акт.docm' },
    ] } });""",
    "distinct template payload expectation",
)

replace_once(
    "await click(/Создать комплекты/);",
    "await click(/Создать комплекты/);\n    await waitFor(() => expect(calls.some((c) => c.command === 'render_mail_merge')).toBe(true));",
    "wait for mail merge rendering",
)

test_path.write_text(payload, encoding="utf-8")

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
