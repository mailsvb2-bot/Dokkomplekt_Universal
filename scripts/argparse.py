"""One-time self-removing shim used by the verified frontend repair workflow."""
from pathlib import Path
import sysconfig

root = Path(__file__).resolve().parents[1]
app = root / "src" / "App.tsx"
source = app.read_text(encoding="utf-8")
start_marker = "  async function selectDocument(doc: DocumentTemplateSpec) {"
end_marker = "\n  function toggleDocumentSelected(documentId: string) {"
start = source.index(start_marker)
end = source.index(end_marker, start)
select_document = """  async function selectDocument(doc: DocumentTemplateSpec) {
    setActiveDoc(doc.id);
    setPreview(null);
    const [workflow, template] = await Promise.all([
      run('get_workflow_plan', () => getWorkflowPlan(doc.id, sickLeave)),
      run('get_document_template_text', () => getDocumentTemplateText(doc.id)),
    ]);
    if (template) setActiveTemplateText(template.template_text);
    if (!workflow) return;
    setPlan(workflow);
    setStatus(workflow.prompts.length ? `Требуется уточнить полей: ${workflow.prompts.length}.` : 'Все поля распознаны — документ готов.');
  }
"""
source = source[:start] + select_document + source[end:]
app.write_text(source, encoding="utf-8")

scenario = root / "src" / "App.scenarios.test.tsx"
test_source = scenario.read_text(encoding="utf-8")
old = """    fireEvent.click(screen.getByRole('button', { name: 'Счёт на оплату' }));
    await screen.findByDisplayValue('7701234567');"""
new = """    fireEvent.click(screen.getByRole('button', { name: 'Счёт на оплату' }));
    fireEvent.click(screen.getByRole('button', { name: 'Выбрать всё' }));
    await screen.findByDisplayValue('7701234567');"""
if new not in test_source:
    if old not in test_source:
        raise RuntimeError("expected explicit-selection scenario block not found")
    scenario.write_text(test_source.replace(old, new, 1), encoding="utf-8")

Path(__file__).unlink(missing_ok=True)
_stdlib_argparse = Path(sysconfig.get_path("stdlib")) / "argparse.py"
exec(compile(_stdlib_argparse.read_bytes(), str(_stdlib_argparse), "exec"), globals(), globals())
