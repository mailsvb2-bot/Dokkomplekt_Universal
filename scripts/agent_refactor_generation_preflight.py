from pathlib import Path

HOOK = r'''import { useState, type Dispatch, type SetStateAction } from 'react';
import type { WorkflowPlan } from '../lib/types';

type PopupAnswer = { field_id: string; value: string; continue_without_value: boolean };
type PopupApplyResult = { accepted: boolean; still_missing?: string[]; message?: string };

interface UseGenerationPreflightOptions {
  selectedDocumentIds: string[];
  preflightPlan: WorkflowPlan | null;
  preflightLoading: boolean;
  answers: Record<string, string>;
  skippedAnswers: Record<string, boolean>;
  setPreflightPlan: Dispatch<SetStateAction<WorkflowPlan | null>>;
  setStatus(message: string): void;
  requestWorkflowPlan(documentIds: string[]): Promise<WorkflowPlan | null | undefined>;
  applyAnswers(documentIds: string[], answers: PopupAnswer[]): Promise<PopupApplyResult | null | undefined>;
  onConfirmed(documentIds: string[]): Promise<void>;
}

/**
 * Owns the universal blocking transition between selecting a document set and
 * publishing it. Professional rules stay in WorkflowPlan; this hook only
 * orchestrates review/confirmation, so the UI cannot become a second rule engine.
 */
export function useGenerationPreflight(options: UseGenerationPreflightOptions) {
  const [generationPreflightOpen, setGenerationPreflightOpen] = useState(false);

  async function openGenerationPreflight() {
    if (!options.selectedDocumentIds.length) {
      options.setStatus('Отметьте хотя бы один документ для комплекта.');
      return;
    }
    if (options.preflightLoading) {
      options.setStatus('Подождите: программа ещё проверяет выбранный комплект.');
      return;
    }
    const workflow = options.preflightPlan ?? await options.requestWorkflowPlan(options.selectedDocumentIds);
    if (!workflow) return;
    options.setPreflightPlan(workflow);
    if (workflow.blocked) {
      options.setStatus(`Создание заблокировано: ${workflow.block_reasons.join('; ')}`);
      return;
    }
    setGenerationPreflightOpen(true);
    options.setStatus('Проверьте данные выбранного комплекта перед созданием.');
  }

  async function confirmGenerationPreflight() {
    const workflow = options.preflightPlan;
    if (!workflow || options.preflightLoading) return;
    if (workflow.blocked) {
      options.setStatus(`Создание заблокировано: ${workflow.block_reasons.join('; ')}`);
      return;
    }
    if (workflow.prompts.length) {
      const missing = workflow.prompts.filter((prompt) => prompt.required
        && !options.skippedAnswers[prompt.field_id]
        && !(options.answers[prompt.field_id] ?? prompt.current_value ?? '').trim());
      if (missing.length) {
        options.setStatus(`Не заполнено обязательное поле: ${missing[0].title}.`);
        return;
      }
      const payload = workflow.prompts.map((prompt) => ({
        field_id: prompt.field_id,
        value: options.skippedAnswers[prompt.field_id] ? '' : options.answers[prompt.field_id] ?? prompt.current_value ?? '',
        continue_without_value: Boolean(options.skippedAnswers[prompt.field_id]),
      }));
      const applied = await options.applyAnswers(options.selectedDocumentIds, payload);
      if (!applied) return;
      if (!applied.accepted) {
        options.setStatus(applied.message || `Не заполнено полей: ${applied.still_missing?.length ?? 0}`);
        return;
      }
    }
    setGenerationPreflightOpen(false);
    options.setStatus('Данные подтверждены. Формируется комплект…');
    await options.onConfirmed(options.selectedDocumentIds);
  }

  return { generationPreflightOpen, setGenerationPreflightOpen, openGenerationPreflight, confirmGenerationPreflight };
}
'''
Path('src/hooks/useGenerationPreflight.ts').write_text(HOOK, encoding='utf-8')

app = Path('src/App.tsx')
text = app.read_text(encoding='utf-8')
anchor = "import { useActionRunner } from './hooks/useActionRunner';\n"
assert text.count(anchor) == 1
text = text.replace(anchor, anchor + "import { useGenerationPreflight } from './hooks/useGenerationPreflight';\n", 1)
text = text.replace("  const [generationPreflightOpen, setGenerationPreflightOpen] = useState(false);\n", "", 1)

start = text.index('  async function generateSelectedDocuments() {')
end = text.index('  async function performGenerateSelectedDocuments', start)
text = text[:start] + text[end:]

hook_anchor = "  async function loadWorkflowPlan(documentIds: string[]): Promise<WorkflowPlan> {\n    return documentIds.length === 1\n      ? getWorkflowPlan(documentIds[0], sickLeave)\n      : getWorkflowPlanBatch(documentIds, sickLeave);\n  }\n"
assert text.count(hook_anchor) == 1
hook_usage = hook_anchor + """
  const { generationPreflightOpen, setGenerationPreflightOpen, openGenerationPreflight, confirmGenerationPreflight } = useGenerationPreflight({
    selectedDocumentIds: selectedDocIds, preflightPlan, preflightLoading, answers, skippedAnswers, setPreflightPlan, setStatus,
    requestWorkflowPlan: (ids) => run(ids.length === 1 ? 'get_workflow_plan' : 'get_workflow_plan_batch', () => loadWorkflowPlan(ids)),
    applyAnswers: (ids, payload) => ids.length === 1
      ? run('apply_popup', () => applyPopup(ids[0], sickLeave, payload))
      : run('apply_popup_batch', () => applyPopupBatch(ids, sickLeave, payload)),
    onConfirmed: performGenerateSelectedDocuments,
  });
"""
text = text.replace(hook_anchor, hook_usage, 1)
text = text.replace('onCreateSelected={generateSelectedDocuments}', 'onCreateSelected={openGenerationPreflight}')
app.write_text(text, encoding='utf-8')
