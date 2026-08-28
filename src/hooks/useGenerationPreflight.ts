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
  onConfirmed(snapshot: GenerationSnapshot): Promise<string | null>;
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
  const [generationError, setGenerationError] = useState<string | null>(null);
  const [generationValidationFieldId, setGenerationValidationFieldId] = useState<string | null>(null);
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
    setGenerationError(null);
    setGenerationValidationFieldId(null);
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
    const reviewedWorkflow = options.preflightPlan;
    const snapshot = generationSnapshot;
    if (!reviewedWorkflow || !snapshot?.documentIds.length || options.preflightLoading || confirmationInFlight.current) return;
    confirmationInFlight.current = true;
    try {
      // Re-read the backend-owned plan at the actual commit boundary. The case can
      // legitimately change between opening the dialog and pressing Create.
      // Submitting prompts from the stale reviewed plan makes the backend reject a
      // now-satisfied field as an "unknown popup answer". The UI still owns no
      // business rules: it submits only the newest server plan.
      const workflow = await options.requestWorkflowPlan(snapshot);
      if (!workflow) {
        const message = 'Не удалось обновить план создания. Комплект не создан.';
        setGenerationError(message);
        setGenerationValidationFieldId(null);
        options.setStatus(message);
        return;
      }
      options.setPreflightPlan(workflow);
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
          const message = `Не заполнено обязательное поле: ${missing[0].title}.`;
          setGenerationError(message);
          setGenerationValidationFieldId(missing[0].field_id);
          options.setStatus(message);
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
          const message = applied.message || `Не заполнено полей: ${applied.still_missing.length}`;
          setGenerationError(message);
          setGenerationValidationFieldId(applied.still_missing[0]?.field_id ?? null);
          options.setStatus(message);
          return;
        }
      }
      setGenerationError(null);
      setGenerationValidationFieldId(null);
      options.setStatus('Данные подтверждены. Формируется комплект…');
      const generationFailure = await options.onConfirmed(snapshot);
      if (generationFailure) {
        setGenerationError(generationFailure);
        setGenerationValidationFieldId(null);
        options.setStatus(generationFailure);
        return;
      }
      setGenerationPreflightOpen(false);
      setGenerationSnapshot(null);
    } finally {
      confirmationInFlight.current = false;
    }
  }

  function closeGenerationPreflight() {
    if (confirmationInFlight.current) return;
    setGenerationPreflightOpen(false);
    setGenerationSnapshot(null);
    setGenerationError(null);
    setGenerationValidationFieldId(null);
  }

  return {
    generationPreflightOpen,
    generationDocumentIds: generationSnapshot?.documentIds ?? [],
    generationSnapshot,
    generationError,
    generationValidationFieldId,
    closeGenerationPreflight,
    openGenerationPreflight,
    confirmGenerationPreflight,
  };
}
