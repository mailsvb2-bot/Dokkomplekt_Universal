import type { Dispatch, SetStateAction } from 'react';
import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { PopupAnswerDto, PopupApplyResult, WorkflowPlan } from '../lib/types';
import { useGenerationPreflight } from './useGenerationPreflight';

const blockedPlan: WorkflowPlan = {
  document_id: 'batch:medical.discharge',
  prompts: [],
  blocked: true,
  block_reasons: ['Небезопасный placeholder: legacy.unknown_field'],
};

describe('useGenerationPreflight', () => {
  it('opens the canonical review dialog for a blocked plan instead of making the create click look dead', async () => {
    const setPreflightPlan = vi.fn() as unknown as Dispatch<SetStateAction<WorkflowPlan | null>>;
    const setStatus = vi.fn();
    const requestWorkflowPlan = vi.fn(async () => blockedPlan);
    const applyAnswers = vi.fn(async (_ids: string[], _answers: PopupAnswerDto[]) => null as PopupApplyResult | null);
    const onConfirmed = vi.fn(async () => undefined);

    const { result } = renderHook(() => useGenerationPreflight({
      selectedDocumentIds: ['medical.discharge'],
      preflightPlan: blockedPlan,
      preflightLoading: false,
      answers: {},
      skippedAnswers: {},
      setPreflightPlan,
      setStatus,
      requestWorkflowPlan,
      applyAnswers,
      onConfirmed,
    }));

    await act(async () => {
      await result.current.openGenerationPreflight();
    });

    expect(result.current.generationPreflightOpen).toBe(true);
    expect(setPreflightPlan).toHaveBeenCalledWith(blockedPlan);
    expect(setStatus).toHaveBeenLastCalledWith(
      'Создание заблокировано: Небезопасный placeholder: legacy.unknown_field',
    );
    expect(onConfirmed).not.toHaveBeenCalled();

    await act(async () => {
      await result.current.confirmGenerationPreflight();
    });

    expect(onConfirmed).not.toHaveBeenCalled();
    expect(applyAnswers).not.toHaveBeenCalled();
    expect(result.current.generationPreflightOpen).toBe(true);
  });

  it('reports a missing backend plan instead of silently returning from the create click', async () => {
    const setPreflightPlan = vi.fn() as unknown as Dispatch<SetStateAction<WorkflowPlan | null>>;
    const setStatus = vi.fn();
    const requestWorkflowPlan = vi.fn(async () => null);

    const { result } = renderHook(() => useGenerationPreflight({
      selectedDocumentIds: ['medical.discharge'],
      preflightPlan: null,
      preflightLoading: false,
      answers: {},
      skippedAnswers: {},
      setPreflightPlan,
      setStatus,
      requestWorkflowPlan,
      applyAnswers: vi.fn(async () => null),
      onConfirmed: vi.fn(async () => undefined),
    }));

    await act(async () => {
      await result.current.openGenerationPreflight();
    });

    expect(result.current.generationPreflightOpen).toBe(false);
    expect(setStatus).toHaveBeenLastCalledWith(
      'Не удалось получить финальный план создания. Комплект не создан.',
    );
  });
  it('does not validate or submit a required prompt hidden by a negative Yes/No dependency', async () => {
    const conditionalPlan: WorkflowPlan = {
      document_id: 'generic.contract',
      prompts: [
        {
          field_id: 'custom.need_details',
          title: 'Нужны дополнительные сведения?',
          required: true,
          input_kind: 'yes_no',
          ask_mode: 'always',
          options: ['Нет', 'Да'],
        },
        {
          field_id: 'custom.details',
          title: 'Дополнительные сведения',
          required: true,
          input_kind: 'long_text',
          ask_mode: 'always',
          linked_to: 'custom.need_details',
        },
      ],
      blocked: false,
      block_reasons: [],
    };
    const applied: PopupApplyResult = {
      accepted: true,
      semantic_case: { values: {}, collections: {}, blocks: {}, skipped_fields: [] },
      still_missing: [],
      message: 'ok',
    };
    const applyAnswers = vi.fn(async (_ids: string[], _answers: PopupAnswerDto[]) => applied);
    const onConfirmed = vi.fn(async () => undefined);

    const { result } = renderHook(() => useGenerationPreflight({
      selectedDocumentIds: ['generic.contract'],
      preflightPlan: conditionalPlan,
      preflightLoading: false,
      answers: { 'custom.need_details': 'Нет' },
      skippedAnswers: {},
      setPreflightPlan: vi.fn() as unknown as Dispatch<SetStateAction<WorkflowPlan | null>>,
      setStatus: vi.fn(),
      requestWorkflowPlan: vi.fn(async () => conditionalPlan),
      applyAnswers,
      onConfirmed,
    }));

    await act(async () => {
      await result.current.confirmGenerationPreflight();
    });

    expect(applyAnswers).toHaveBeenCalledWith(
      ['generic.contract'],
      [{ field_id: 'custom.need_details', value: 'Нет', continue_without_value: false }],
    );
    expect(onConfirmed).toHaveBeenCalledWith(['generic.contract']);
  });

});
