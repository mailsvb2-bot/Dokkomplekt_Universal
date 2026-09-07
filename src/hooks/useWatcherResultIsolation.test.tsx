import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { DocumentTemplateSpec } from '../lib/types';

let listener: ((event: { payload: unknown }) => void) | null = null;
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_name: string, callback: (event: { payload: unknown }) => void) => {
    listener = callback;
    return () => { listener = null; };
  }),
}));

import { useWatcherResultIsolation } from './useWatcherResultIsolation';

const document: DocumentTemplateSpec = {
  id: 'doc', button_label: 'Документ', template_path: 'doc.docx', category: 'Generic', role_id: 'generic',
  required_fields: [], placeholders: [], is_static_copy: false,
};
const processed = {
  status: 'processed', patient_folder: 'C:/Ready/Case-B', created_files: ['C:/Ready/Case-B/doc.docx'],
  created_documents: [{ document_id: 'doc', label: 'Документ', path: 'C:/Ready/Case-B/doc.docx' }],
  missing: [], attention_file: null, message: 'Комплект Б готов.',
};

describe('useWatcherResultIsolation', () => {
  beforeEach(() => { listener = null; });

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
