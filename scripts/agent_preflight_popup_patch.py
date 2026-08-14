from pathlib import Path

COMPONENT = r'''import type { Dispatch, SetStateAction } from 'react';
import type { DocumentTemplateSpec, PromptSpec, WorkflowPlan } from '../lib/types';
import { WorkflowPromptField } from './Workspace';

interface GenerationPreflightModalProps {
  plan: WorkflowPlan;
  documents: DocumentTemplateSpec[];
  selectedDocumentIds: string[];
  answers: Record<string, string>;
  skippedAnswers: Record<string, boolean>;
  busy: boolean;
  loading: boolean;
  showSickLeaveOption: boolean;
  sickLeaveEnabled: boolean;
  setAnswers: Dispatch<SetStateAction<Record<string, string>>>;
  setSkippedAnswers: Dispatch<SetStateAction<Record<string, boolean>>>;
  onSickLeaveChange(value: boolean): void;
  onCancel(): void;
  onConfirm(): void;
}

export function GenerationPreflightModal(props: GenerationPreflightModalProps) {
  const selected = props.documents.filter((document) => props.selectedDocumentIds.includes(document.id));
  const prompts = props.plan.prompts;
  const sections = prompts.reduce<Array<{ title: string; prompts: PromptSpec[] }>>((groups, prompt) => {
    const title = prompt.section?.trim() || 'Данные документа';
    const existing = groups.find((group) => group.title === title);
    if (existing) existing.prompts.push(prompt);
    else groups.push({ title, prompts: [prompt] });
    return groups;
  }, []);

  function changePrompt(prompt: PromptSpec, value: string) {
    props.setSkippedAnswers((previous) => ({ ...previous, [prompt.field_id]: false }));
    props.setAnswers((previous) => {
      const previousSourceValue = previous[prompt.field_id] ?? prompt.current_value ?? '';
      const next = { ...previous, [prompt.field_id]: value };
      for (const linkedPrompt of prompts) {
        if (linkedPrompt.linked_to !== prompt.field_id) continue;
        const linkedCurrent = previous[linkedPrompt.field_id] ?? linkedPrompt.current_value ?? '';
        if (!linkedCurrent || linkedCurrent === previousSourceValue) next[linkedPrompt.field_id] = value;
      }
      return next;
    });
  }

  return (
    <div className="backdrop" role="dialog" aria-modal="true" aria-labelledby="generation-preflight-title">
      <div className="modal popupDesignerModal generationPreflightModal">
        <h2 id="generation-preflight-title">Проверка перед созданием</h2>
        <p className="hint">
          Проверьте данные именно для выбранного комплекта. Значения, найденные в источнике, уже подставлены; реквизиты создаваемого документа подтверждаются здесь перед публикацией.
        </p>

        <div className="readyMessage">
          <i className="ti ti-files" aria-hidden="true" />
          <div>
            <strong>Будет создано: {selected.length}</strong>
            <span>{selected.map((document) => document.button_label).join(' · ') || 'Выбранный комплект'}</span>
          </div>
        </div>

        {props.plan.blocked ? (
          <div className="readyMessage notReady" role="alert">
            <i className="ti ti-alert-triangle" aria-hidden="true" />
            <div><strong>Создание заблокировано</strong><span>{props.plan.block_reasons.join('; ')}</span></div>
          </div>
        ) : null}

        {props.showSickLeaveOption ? (
          <label className="checkLine workflowOption">
            <input type="checkbox" checked={props.sickLeaveEnabled} onChange={(event) => props.onSickLeaveChange(event.target.checked)} />
            <span>Оформляется больничный лист</span>
          </label>
        ) : null}

        {sections.length ? sections.map((section) => (
          <section key={section.title} className="clientFields" aria-label={section.title}>
            <h3>{section.title}</h3>
            {section.prompts.map((prompt) => (
              <WorkflowPromptField
                key={prompt.field_id}
                prompt={prompt}
                value={props.answers[prompt.field_id] ?? prompt.current_value ?? ''}
                skipped={Boolean(props.skippedAnswers[prompt.field_id])}
                onChange={(value) => changePrompt(prompt, value)}
                onSkipChange={(skipped) => props.setSkippedAnswers((previous) => ({ ...previous, [prompt.field_id]: skipped }))}
                onPin={() => undefined}
                showPin={false}
              />
            ))}
          </section>
        )) : (
          <div className="readyMessage">
            <i className="ti ti-circle-check" aria-hidden="true" />
            <div><strong>Все обязательные данные уже найдены</strong><span>Можно подтвердить создание комплекта.</span></div>
          </div>
        )}

        <div className="modalActions">
          <button className="softBtn" type="button" onClick={props.onCancel} disabled={props.busy}>Отмена</button>
          <span className="spacer" />
          <button className="primaryBtn" type="button" onClick={props.onConfirm} disabled={props.busy || props.loading || props.plan.blocked}>
            {props.loading ? 'Проверяем сценарий…' : props.busy ? 'Создаём документы…' : 'Создать документы'}
          </button>
        </div>
      </div>
    </div>
  );
}
'''

workspace = Path('src/components/Workspace.tsx')
text = workspace.read_text(encoding='utf-8')
old = """function WorkflowPromptField(props: {
  prompt: PromptSpec;
  value: string;
  skipped: boolean;
  onChange(value: string): void;
  onSkipChange(value: boolean): void;
  onPin(): void;
}) {"""
new = """export function WorkflowPromptField(props: {
  prompt: PromptSpec;
  value: string;
  skipped: boolean;
  onChange(value: string): void;
  onSkipChange(value: boolean): void;
  onPin(): void;
  showPin?: boolean;
}) {"""
assert text.count(old) == 1
text = text.replace(old, new, 1)
old_pin = '        <button type="button" className="iconOnlyBtn" disabled={props.skipped} title="Использовать это значение во всех документах комплекта" aria-label={`Использовать ${prompt.title} во всех документах`} onClick={props.onPin}><i className="ti ti-pin" aria-hidden="true" /></button>'
new_pin = '        {props.showPin !== false ? <button type="button" className="iconOnlyBtn" disabled={props.skipped} title="Использовать это значение во всех документах комплекта" aria-label={`Использовать ${prompt.title} во всех документах`} onClick={props.onPin}><i className="ti ti-pin" aria-hidden="true" /></button> : null}'
assert text.count(old_pin) == 1
workspace.write_text(text.replace(old_pin, new_pin, 1), encoding='utf-8')

Path('src/components/GenerationPreflightModal.tsx').write_text(COMPONENT, encoding='utf-8')

app = Path('src/App.tsx')
text = app.read_text(encoding='utf-8')
import_anchor = "import { GuidedScannerModal } from './components/GuidedScannerModal';\n"
assert text.count(import_anchor) == 1
text = text.replace(import_anchor, import_anchor + "import { GenerationPreflightModal } from './components/GenerationPreflightModal';\n", 1)
state_anchor = "  const [preflightLoading, setPreflightLoading] = useState(false);\n"
assert text.count(state_anchor) == 1
text = text.replace(state_anchor, state_anchor + "  const [generationPreflightOpen, setGenerationPreflightOpen] = useState(false);\n", 1)

start = text.index('  async function generateSelectedDocuments() {')
end = text.index('  async function performGenerateSelectedDocuments', start)
replacement = '''  async function generateSelectedDocuments() {
    if (!selectedDocIds.length) {
      setStatus('Отметьте хотя бы один документ для комплекта.');
      return;
    }
    if (preflightLoading) {
      setStatus('Подождите: программа ещё проверяет выбранный комплект.');
      return;
    }

    const workflow = preflightPlan ?? await run(
      selectedDocIds.length === 1 ? 'get_workflow_plan' : 'get_workflow_plan_batch',
      () => loadWorkflowPlan(selectedDocIds),
    );
    if (!workflow) return;
    setPreflightPlan(workflow);
    if (workflow.blocked) {
      setStatus(`Создание заблокировано: ${workflow.block_reasons.join('; ')}`);
      return;
    }
    setGenerationPreflightOpen(true);
    setStatus('Проверьте данные выбранного комплекта перед созданием.');
  }

  async function confirmGenerationPreflight() {
    const workflow = preflightPlan;
    if (!workflow || preflightLoading) return;
    if (workflow.blocked) {
      setStatus(`Создание заблокировано: ${workflow.block_reasons.join('; ')}`);
      return;
    }

    if (workflow.prompts.length) {
      const missing = workflow.prompts.filter((prompt) => prompt.required && !skippedAnswers[prompt.field_id] && !(answers[prompt.field_id] ?? prompt.current_value ?? '').trim());
      if (missing.length) {
        setStatus(`Не заполнено обязательное поле: ${missing[0].title}.`);
        return;
      }
      const payload = workflow.prompts.map((prompt) => ({
        field_id: prompt.field_id,
        value: skippedAnswers[prompt.field_id] ? '' : answers[prompt.field_id] ?? prompt.current_value ?? '',
        continue_without_value: Boolean(skippedAnswers[prompt.field_id]),
      }));
      const applied = selectedDocIds.length === 1
        ? await run('apply_popup', () => applyPopup(selectedDocIds[0], sickLeave, payload))
        : await run('apply_popup_batch', () => applyPopupBatch(selectedDocIds, sickLeave, payload));
      if (!applied) return;
      if (!applied.accepted) {
        setStatus(applied.message || `Не заполнено полей: ${applied.still_missing?.length ?? 0}`);
        return;
      }
    }

    setGenerationPreflightOpen(false);
    setStatus('Данные подтверждены. Формируется комплект…');
    await performGenerateSelectedDocuments(selectedDocIds);
  }

'''
text = text[:start] + replacement + text[end:]
modal_anchor = "      {popupDesignerDocument && (\n"
assert text.count(modal_anchor) == 1
modal = '''      {generationPreflightOpen && preflightPlan && (
        <GenerationPreflightModal
          plan={preflightPlan}
          documents={documents}
          selectedDocumentIds={selectedDocIds}
          answers={answers}
          skippedAnswers={skippedAnswers}
          busy={busy}
          loading={preflightLoading}
          showSickLeaveOption={showSickLeaveOption}
          sickLeaveEnabled={sickLeave}
          setAnswers={setAnswers}
          setSkippedAnswers={setSkippedAnswers}
          onSickLeaveChange={setSickLeave}
          onCancel={() => setGenerationPreflightOpen(false)}
          onConfirm={() => void confirmGenerationPreflight()}
        />
      )}

'''
text = text.replace(modal_anchor, modal + modal_anchor, 1)
app.write_text(text, encoding='utf-8')
