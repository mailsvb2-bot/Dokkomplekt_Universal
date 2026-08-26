import type { Dispatch, SetStateAction } from 'react';
import { useEffect } from 'react';
import type { DocumentTemplateSpec, PromptSpec, WorkflowPlan } from '../lib/types';
import { activeWorkflowPrompts } from '../lib/workflowPromptVisibility';
import { WorkflowPromptField } from './Workspace';

const INTERNAL_DIARY_RUNTIME_FIELDS = new Set([
  'medical.diary_day_start_time',
  'medical.diary_day_end_time',
]);

interface GenerationPreflightModalProps {
  plan: WorkflowPlan;
  documents: DocumentTemplateSpec[];
  selectedDocumentIds: string[];
  answers: Record<string, string>;
  skippedAnswers: Record<string, boolean>;
  busy: boolean;
  loading: boolean;
  generationError: string | null;
  invalidFieldId: string | null;
  showSickLeaveOption: boolean;
  sickLeaveEnabled: boolean;
  setAnswers: Dispatch<SetStateAction<Record<string, string>>>;
  setSkippedAnswers: Dispatch<SetStateAction<Record<string, boolean>>>;
  onSickLeaveChange(value: boolean): void;
  onCancel(): void;
  onConfirm(): void;
}

export function GenerationPreflightModal(props: GenerationPreflightModalProps) {
  useEffect(() => {
    if (!props.invalidFieldId) return;
    const inputId = `workflow-${props.invalidFieldId.replace(/[^a-zA-Z0-9_-]/g, '-')}`;
    const control = document.getElementById(inputId) as HTMLElement | null;
    control?.scrollIntoView?.({ block: 'center', behavior: 'smooth' });
    control?.focus();
  }, [props.invalidFieldId]);
  const selected = props.documents.filter((document) => props.selectedDocumentIds.includes(document.id));
  const prompts = activeWorkflowPrompts(props.plan.prompts, props.answers);
  // These values are backend-owned bounds for the generic repeated-record engine.
  // They remain in the WorkflowPlan and are submitted by useGenerationPreflight,
  // but are not extra user questions. Donor-facing choices stay in the WorkflowPlan; only these
  // technical bounds are hidden from the specialist.
  const visiblePrompts = prompts.filter((prompt) => !INTERNAL_DIARY_RUNTIME_FIELDS.has(prompt.field_id));
  const sections = visiblePrompts.reduce<Array<{ title: string; prompts: PromptSpec[] }>>((groups, prompt) => {
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
        // A linked field whose source is Yes/No is a generic visibility dependency. It must not
        // receive the literal source value ("Да"/"Нет"). Date-to-date links keep copy behavior.
        if (prompt.input_kind === 'yes_no') continue;
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

        {props.generationError ? (
          <div className="readyMessage notReady generationActionError" role="alert" data-testid="generation-error">
            <i className="ti ti-alert-triangle" aria-hidden="true" />
            <div><strong>Документы не созданы</strong><span>{props.generationError}</span></div>
          </div>
        ) : null}

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
