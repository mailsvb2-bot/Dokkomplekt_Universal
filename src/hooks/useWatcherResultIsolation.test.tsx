import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { CreatedDocumentsIntakeResult, DocumentTemplateSpec } from '../lib/types';

let listener: ((event: { payload: unknown }) => void) | null = null;
let listenCalls = 0;
let unlistenCalls = 0;
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_name: string, callback: (event: { payload: unknown }) => void) => {
    listenCalls += 1;
    listener = callback;
    return () => { unlistenCalls += 1; listener = null; };
  }),
}));

import { watcherForegroundCaseActive, useWatcherResultIsolation } from './useWatcherResultIsolation';

const document: DocumentTemplateSpec = {
  id: 'doc', button_label: 'Документ', template_path: 'doc.docx', category: 'Generic', role_id: 'generic',
  required_fields: [], placeholders: [], is_static_copy: false,
};
const processed: CreatedDocumentsIntakeResult = {
  status: 'processed', patient_folder: 'C:/Ready/Case-B', created_files: ['C:/Ready/Case-B/doc.docx'],
  created_documents: [{ document_id: 'doc', label: 'Документ', path: 'C:/Ready/Case-B/doc.docx' }],
  missing: [], attention_file: null, message: 'Комплект Б готов.',
};

describe('watcherForegroundCaseActive', () => {
  const base = { sourceFileName: null, hasParsedSource: false, sourceText: '', intakeSource: '', intakeResult: null, lastOutput: null };

  it('treats zero-touch activity and results as foreground ownership', () => {
    expect(watcherForegroundCaseActive({ ...base, intakeSource: 'C:/Input/case.docx' })).toBe(true);
    expect(watcherForegroundCaseActive({ ...base, intakeResult: processed })).toBe(true);
    expect(watcherForegroundCaseActive({ ...base, lastOutput: { folder: 'C:/Ready/Case-B', files: ['C:/Ready/Case-B/doc.docx'], source: 'zero_touch', print_items: [] } })).toBe(true);
    expect(watcherForegroundCaseActive(base)).toBe(false);
  });
});

describe('useWatcherResultIsolation', () => {
  beforeEach(() => { listener = null; listenCalls = 0; unlistenCalls = 0; });

  it('never replaces the open foreground case with an unrelated watcher result', async () => {
    const setLastOutput = vi.fn();
    const setIntakeResult = vi.fn();
    const setStatus = vi.fn();
    const { result } = renderHook(() => useWatcherResultIsolation({
      documents: [document], foregroundCaseActive: true, setLastOutput, setIntakeResult, setStatus,
    }));
    await waitFor(() => expect(listener).not.toBeNull());
    act(() => listener?.({ payload: processed }));
    expect(setLastOutput).not.toHaveBeenCalled();
    expect(setIntakeResult).not.toHaveBeenCalled();
    expect(setStatus).not.toHaveBeenCalled();
    expect(result.current.backgroundNotice).toContain('Текущий открытый комплект не изменён');
  });

  it('keeps one listener across rerenders and reads the current foreground state', async () => {
    const setLastOutput = vi.fn();
    const setIntakeResult = vi.fn();
    const setStatus = vi.fn();
    const { result, rerender, unmount } = renderHook(({ active }) => useWatcherResultIsolation({
      documents: [document], foregroundCaseActive: active, setLastOutput, setIntakeResult, setStatus,
    }), { initialProps: { active: false } });
    await waitFor(() => expect(listener).not.toBeNull());
    expect(listenCalls).toBe(1);

    rerender({ active: true });
    expect(listenCalls).toBe(1);
    act(() => listener?.({ payload: processed }));
    expect(setIntakeResult).not.toHaveBeenCalled();
    expect(result.current.backgroundNotice).toContain('Текущий открытый комплект не изменён');

    rerender({ active: false });
    expect(listenCalls).toBe(1);
    act(() => listener?.({ payload: processed }));
    expect(setIntakeResult).toHaveBeenCalledTimes(1);
    expect(setStatus).toHaveBeenCalledWith('Комплект Б готов.');
    unmount();
    expect(unlistenCalls).toBe(1);
  });

  it('publishes watcher output normally when no foreground case is open', async () => {
    const setLastOutput = vi.fn();
    const setIntakeResult = vi.fn();
    const setStatus = vi.fn();
    renderHook(() => useWatcherResultIsolation({
      documents: [document], foregroundCaseActive: false, setLastOutput, setIntakeResult, setStatus,
    }));
    await waitFor(() => expect(listener).not.toBeNull());
    act(() => listener?.({ payload: processed }));
    expect(setIntakeResult).toHaveBeenCalledWith(expect.objectContaining({ status: 'processed' }));
    expect(setStatus).toHaveBeenCalledWith('Комплект Б готов.');
    expect(setLastOutput).toHaveBeenLastCalledWith(expect.objectContaining({ source: 'watcher', folder: 'C:/Ready/Case-B' }));
  });
});
