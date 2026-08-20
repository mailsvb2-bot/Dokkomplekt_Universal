import { useRef, useState, type Dispatch, type SetStateAction } from 'react';
import type { FolderNamePartDto, PopupAnswerDto, PopupApplyResult, WorkflowPlan } from '../lib/types';
import { activeWorkflowPrompts } from '../lib/workflowPromptVisibility';

export interface GenerationSnapshot {
  documentIds: string[];
  sickLeaveEnabled: boolean;
  folderParts: FolderNamePartDto[];
  outputRoot: string;
  documentRevisionTokens: Record<string, string>;
}

interface UseGenerationPreflightOptions {
  selectedDocumentIds: string[];
  sickLeaveEnabled: boolean;
  folderParts: FolderNamePartDto[];
  outputRoot: string;
  documentRevisionTokens: Record<string, string>;
  preflightPlan: WorkflowPlan | null;
  preflightLoading: boolean;
  answers: Record<string, string>;
  skippedAnswers: Record<string, boolean>;
  setPreflightPlan: Dispatch<SetStateAction<WorkflowPlan | null>>;
  setStatus(message: string): void;
  requestWorkflowPlan(snapshot: GenerationSnapshot): Promise<WorkflowPlan | null | undefined>;
  applyAnswers(snapshot: GenerationSnapshot, answers: PopupAnswerDto[]): Promise<PopupApplyResult | null | undefined>;
  onConfirmed(snapshot: GenerationSnapshot): Promise<void>;
}

/**
 * Owns the universal blocking transition between selecting a document set and
 * publishing it. Professional rules stay in WorkflowPlan; this hook only
 * orchestrates review/confirmation, so the UI cannot become a second rule engine.
 *
 * The donor applications bind one entire create action to one reviewed state.
 * Universal therefore snapshots every input that can change the workflow or the
 * publication destination, not only document IDs.
 */
export function useGenerationPreflight(options: UseGenerationPreflightOptions) {
  const [generationPreflightOpen, setGenerationPreflightOpen] = useState(false);
  const [generationSnapshot, setGenerationSnapshot] = useState<GenerationSnapshot | null>(null);
  const confirmationInFlight = useRef(false);

  async function openGenerationPreflight() {
    if (!options.selectedDocumentIds.length) {
      options.setStatus('Отметьте хотя бы один документ для комплекта.');
      return;
    }
    if (options.preflightLoading || confirmationInFlight.current) {
      options.setStatus('Подождите: программа ещё проверяет выбранный комплект.');
      return;
    }
    const snapshot: GenerationSnapshot = {
      documentIds: [...options.selectedDocumentIds],
      sickLeaveEnabled: options.sickLeaveEnabled,
      folderParts: [...options.folderParts],
      outputRoot: options.outputRoot.trim(),
      documentRevisionTokens: { ...options.documentRevisionTokens },
    };
    const workflow = await options.requestWorkflowPlan(snapshot);
    if (!workflow) {
      options.setStatus('Не удалось получить финальный план создания. Комплект не создан.');
      return;
    }
    setGenerationSnapshot(snapshot);
    options.setPreflightPlan(workflow);
    setGenerationPreflightOpen(true);
    if (workflow.blocked) {
      options.setStatus(`Создание заблокировано: ${workflow.block_reasons.join('; ')}`);
      return;
    }
    options.setStatus('Проверьте данные выбранного комплекта перед созданием.');
  }

  async function confirmGenerationPreflight() {
    const workflow = options.preflightPlan;
    const snapshot = generationSnapshot;
    if (!workflow || !snapshot?.documentIds.length || options.preflightLoading || confirmationInFlight.current) return;
    confirmationInFlight.current = true;
    try {
      if (workflow.blocked) {
        options.setStatus(`Создание заблокировано: ${workflow.block_reasons.join('; ')}`);
        return;
      }
      if (workflow.prompts.length) {
        const activePrompts = activeWorkflowPrompts(workflow.prompts, options.answers);
        const missing = activePrompts.filter((prompt) => prompt.required
          && !options.skippedAnswers[prompt.field_id]
          && !(options.answers[prompt.field_id] ?? prompt.current_value ?? '').trim());
        if (missing.length) {
          options.setStatus(`Не заполнено обязательное поле: ${missing[0].title}.`);
          return;
        }
        const payload: PopupAnswerDto[] = activePrompts.map((prompt) => ({
          field_id: prompt.field_id,
          value: options.skippedAnswers[prompt.field_id] ? '' : options.answers[prompt.field_id] ?? prompt.current_value ?? '',
          continue_without_value: Boolean(options.skippedAnswers[prompt.field_id]),
        }));
        const applied = await options.applyAnswers(snapshot, payload);
        if (!applied) return;
        if (!applied.accepted) {
          options.setStatus(applied.message || `Не заполнено полей: ${applied.still_missing.length}`);
          return;
        }
      }
      setGenerationPreflightOpen(false);
      options.setStatus('Данные подтверждены. Формируется комплект…');
      await options.onConfirmed(snapshot);
      setGenerationSnapshot(null);
    } finally {
      confirmationInFlight.current = false;
    }
  }

  function closeGenerationPreflight() {
    if (confirmationInFlight.current) return;
    setGenerationPreflightOpen(false);
    setGenerationSnapshot(null);
  }

  return {
    generationPreflightOpen,
    generationDocumentIds: generationSnapshot?.documentIds ?? [],
    generationSnapshot,
    closeGenerationPreflight,
    openGenerationPreflight,
    confirmGenerationPreflight,
  };
}
