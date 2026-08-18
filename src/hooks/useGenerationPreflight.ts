import { useState, type Dispatch, type SetStateAction } from 'react';
import type { PopupAnswerDto, PopupApplyResult, WorkflowPlan } from '../lib/types';

interface UseGenerationPreflightOptions {
  selectedDocumentIds: string[];
  preflightPlan: WorkflowPlan | null;
  preflightLoading: boolean;
  answers: Record<string, string>;
  skippedAnswers: Record<string, boolean>;
  setPreflightPlan: Dispatch<SetStateAction<WorkflowPlan | null>>;
  setStatus(message: string): void;
  requestWorkflowPlan(documentIds: string[]): Promise<WorkflowPlan | null | undefined>;
  applyAnswers(documentIds: string[], answers: PopupAnswerDto[]): Promise<PopupApplyResult | null | undefined>;
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
    if (!workflow) {
      options.setStatus('Не удалось получить финальный план создания. Комплект не создан.');
      return;
    }
    options.setPreflightPlan(workflow);

    // A click on the primary generation action must never look like a no-op.
    // Keep backend blockers fail-closed, but always open the canonical preflight
    // so the specialist can see the exact reason and fix the template/data.
    setGenerationPreflightOpen(true);
    if (workflow.blocked) {
      options.setStatus(`Создание заблокировано: ${workflow.block_reasons.join('; ')}`);
      return;
    }
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
      const payload: PopupAnswerDto[] = workflow.prompts.map((prompt) => ({
        field_id: prompt.field_id,
        value: options.skippedAnswers[prompt.field_id] ? '' : options.answers[prompt.field_id] ?? prompt.current_value ?? '',
        continue_without_value: Boolean(options.skippedAnswers[prompt.field_id]),
      }));
      const applied = await options.applyAnswers(options.selectedDocumentIds, payload);
      if (!applied) return;
      if (!applied.accepted) {
        options.setStatus(applied.message || `Не заполнено полей: ${applied.still_missing.length}`);
        return;
      }
    }
    setGenerationPreflightOpen(false);
    options.setStatus('Данные подтверждены. Формируется комплект…');
    await options.onConfirmed(options.selectedDocumentIds);
  }

  return { generationPreflightOpen, setGenerationPreflightOpen, openGenerationPreflight, confirmGenerationPreflight };
}
