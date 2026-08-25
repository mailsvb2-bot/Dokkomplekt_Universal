import type { Dispatch, SetStateAction } from 'react';
import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FolderNamePartDto, PopupAnswerDto, PopupApplyResult, WorkflowPlan } from '../lib/types';
import { useGenerationPreflight, type GenerationSnapshot } from './useGenerationPreflight';

const blockedPlan: WorkflowPlan = {
  document_id: 'batch:medical.discharge', prompts: [], blocked: true,
  block_reasons: ['Небезопасный placeholder: legacy.unknown_field'],
};

function context(documentId: string) {
  return {
    sickLeaveEnabled: false,
    folderParts: ['DocumentNumber'] as FolderNamePartDto[],
    outputRoot: 'C:/Desktop/Выписанные пациенты',
    documentRevisionTokens: { [documentId]: 'revision-1' },
  };
}

describe('useGenerationPreflight', () => {
  it('opens the canonical review dialog for a blocked plan instead of making the create click look dead', async () => {
    const setPreflightPlan = vi.fn() as unknown as Dispatch<SetStateAction<WorkflowPlan | null>>;
    const setStatus = vi.fn();
    const requestWorkflowPlan = vi.fn(async (_snapshot: GenerationSnapshot) => blockedPlan);
    const applyAnswers = vi.fn(async (_snapshot: GenerationSnapshot, _answers: PopupAnswerDto[]) => null as PopupApplyResult | null);
    const onConfirmed = vi.fn(async (_snapshot: GenerationSnapshot) => null);
    const { result } = renderHook(() => useGenerationPreflight({
      selectedDocumentIds: ['medical.discharge'], ...context('medical.discharge'), preflightPlan: blockedPlan,
      preflightLoading: false, answers: {}, skippedAnswers: {}, setPreflightPlan, setStatus,
      requestWorkflowPlan, applyAnswers, onConfirmed,
    }));

    await act(async () => { await result.current.openGenerationPreflight(); });
    expect(result.current.generationPreflightOpen).toBe(true);
    expect(requestWorkflowPlan).toHaveBeenCalledWith(expect.objectContaining({ documentIds: ['medical.discharge'] }));
    expect(setPreflightPlan).toHaveBeenCalledWith(blockedPlan);
    expect(setStatus).toHaveBeenLastCalledWith('Создание заблокировано: Небезопасный placeholder: legacy.unknown_field');
    expect(onConfirmed).not.toHaveBeenCalled();
    await act(async () => { await result.current.confirmGenerationPreflight(); });
    expect(onConfirmed).not.toHaveBeenCalled();
    expect(applyAnswers).not.toHaveBeenCalled();
    expect(result.current.generationPreflightOpen).toBe(true);
  });

  it('reports a missing backend plan instead of silently returning from the create click', async () => {
    const setPreflightPlan = vi.fn() as unknown as Dispatch<SetStateAction<WorkflowPlan | null>>;
    const setStatus = vi.fn();
    const requestWorkflowPlan = vi.fn(async (_snapshot: GenerationSnapshot) => null);
    const { result } = renderHook(() => useGenerationPreflight({
      selectedDocumentIds: ['medical.discharge'], ...context('medical.discharge'), preflightPlan: null,
      preflightLoading: false, answers: {}, skippedAnswers: {}, setPreflightPlan, setStatus,
      requestWorkflowPlan, applyAnswers: vi.fn(async () => null), onConfirmed: vi.fn(async () => null),
    }));
    await act(async () => { await result.current.openGenerationPreflight(); });
    expect(result.current.generationPreflightOpen).toBe(false);
    expect(setStatus).toHaveBeenLastCalledWith('Не удалось получить финальный план создания. Комплект не создан.');
  });

  it('does not validate or submit a required prompt hidden by a negative Yes/No dependency', async () => {
    const conditionalPlan: WorkflowPlan = {
      document_id: 'generic.contract',
      prompts: [
        { field_id: 'custom.need_details', title: 'Нужны дополнительные сведения?', required: true, input_kind: 'yes_no', ask_mode: 'always', options: ['Нет', 'Да'] },
        { field_id: 'custom.details', title: 'Дополнительные сведения', required: true, input_kind: 'long_text', ask_mode: 'always', linked_to: 'custom.need_details' },
      ], blocked: false, block_reasons: [],
    };
    const applied: PopupApplyResult = { accepted: true, semantic_case: { values: {}, collections: {}, blocks: {}, skipped_fields: [] }, still_missing: [], message: 'ok' };
    const applyAnswers = vi.fn(async (_snapshot: GenerationSnapshot, _answers: PopupAnswerDto[]) => applied);
    const onConfirmed = vi.fn(async (_snapshot: GenerationSnapshot) => null);
    const { result } = renderHook(() => useGenerationPreflight({
      selectedDocumentIds: ['generic.contract'], ...context('generic.contract'), preflightPlan: conditionalPlan,
      preflightLoading: false, answers: { 'custom.need_details': 'Нет' }, skippedAnswers: {},
      setPreflightPlan: vi.fn() as unknown as Dispatch<SetStateAction<WorkflowPlan | null>>, setStatus: vi.fn(),
      requestWorkflowPlan: vi.fn(async () => conditionalPlan), applyAnswers, onConfirmed,
    }));
    await act(async () => { await result.current.openGenerationPreflight(); });
    await act(async () => { await result.current.confirmGenerationPreflight(); });
    expect(applyAnswers).toHaveBeenCalledWith(
      expect.objectContaining({ documentIds: ['generic.contract'] }),
      [{ field_id: 'custom.need_details', value: 'Нет', continue_without_value: false }],
    );
    expect(onConfirmed).toHaveBeenCalledWith(expect.objectContaining({ documentIds: ['generic.contract'] }));
  });

  it('binds popup and publication to the same immutable donor-style generation snapshot', async () => {
    let outputRoot = 'C:/Desktop/Выписанные пациенты';
    let sickLeaveEnabled = false;
    const readyPlan: WorkflowPlan = { document_id: 'contract', prompts: [], blocked: false, block_reasons: [] };
    const onConfirmed = vi.fn(async (_snapshot: GenerationSnapshot) => null);
    const { result, rerender } = renderHook(() => useGenerationPreflight({
      selectedDocumentIds: ['contract'], sickLeaveEnabled, folderParts: ['DocumentNumber'], outputRoot,
      documentRevisionTokens: { contract: 'revision-1' }, preflightPlan: readyPlan, preflightLoading: false,
      answers: {}, skippedAnswers: {}, setPreflightPlan: vi.fn() as unknown as Dispatch<SetStateAction<WorkflowPlan | null>>,
      setStatus: vi.fn(), requestWorkflowPlan: vi.fn(async () => readyPlan), applyAnswers: vi.fn(async () => null), onConfirmed,
    }));
    await act(async () => { await result.current.openGenerationPreflight(); });
    outputRoot = 'D:/changed-after-review'; sickLeaveEnabled = true; rerender();
    await act(async () => { await result.current.confirmGenerationPreflight(); });
    expect(onConfirmed).toHaveBeenCalledWith(expect.objectContaining({
      documentIds: ['contract'], outputRoot: 'C:/Desktop/Выписанные пациенты', sickLeaveEnabled: false,
      folderParts: ['DocumentNumber'], documentRevisionTokens: { contract: 'revision-1' },
    }));
  });

  it('keeps the preflight open and exposes a generation failure instead of looking like a dead button', async () => {
    const readyPlan: WorkflowPlan = { document_id: 'contract', prompts: [], blocked: false, block_reasons: [] };
    const failure = 'Не удалось создать документы: лимит или файловая публикация не прошли.';
    const setStatus = vi.fn();
    const { result } = renderHook(() => useGenerationPreflight({
      selectedDocumentIds: ['contract'], ...context('contract'), preflightPlan: readyPlan, preflightLoading: false,
      answers: {}, skippedAnswers: {}, setPreflightPlan: vi.fn() as unknown as Dispatch<SetStateAction<WorkflowPlan | null>>,
      setStatus, requestWorkflowPlan: vi.fn(async () => readyPlan), applyAnswers: vi.fn(async () => null),
      onConfirmed: vi.fn(async () => failure),
    }));
    await act(async () => { await result.current.openGenerationPreflight(); });
    await act(async () => { await result.current.confirmGenerationPreflight(); });
    expect(result.current.generationPreflightOpen).toBe(true);
    expect(result.current.generationError).toBe(failure);
    expect(setStatus).toHaveBeenLastCalledWith(failure);
  });

  it('ignores a duplicate confirm while one generation is already in flight', async () => {
    const readyPlan: WorkflowPlan = { document_id: 'contract', prompts: [], blocked: false, block_reasons: [] };
    let release!: () => void;
    const pending = new Promise<string | null>((resolve) => { release = () => resolve(null); });
    const onConfirmed = vi.fn(async (_snapshot: GenerationSnapshot) => pending);
    const { result } = renderHook(() => useGenerationPreflight({
      selectedDocumentIds: ['contract'], ...context('contract'), preflightPlan: readyPlan, preflightLoading: false,
      answers: {}, skippedAnswers: {}, setPreflightPlan: vi.fn() as unknown as Dispatch<SetStateAction<WorkflowPlan | null>>,
      setStatus: vi.fn(), requestWorkflowPlan: vi.fn(async () => readyPlan), applyAnswers: vi.fn(async () => null), onConfirmed,
    }));
    await act(async () => { await result.current.openGenerationPreflight(); });
    let first!: Promise<void>;
    await act(async () => { first = result.current.confirmGenerationPreflight(); await Promise.resolve(); });
    await act(async () => { await result.current.confirmGenerationPreflight(); });
    expect(onConfirmed).toHaveBeenCalledTimes(1);
    release();
    await act(async () => { await first; });
  });
});
