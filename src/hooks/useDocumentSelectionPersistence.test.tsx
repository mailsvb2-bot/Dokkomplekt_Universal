import { useState } from 'react';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { setDocumentSelection } from '../lib/api';
import { useDocumentSelectionPersistence } from './useDocumentSelectionPersistence';

vi.mock('../lib/api', () => ({ setDocumentSelection: vi.fn() }));

function useSelection() {
  const [selectedDocumentIds, setSelectedDocumentIds] = useState<string[]>([]);
  const [status, setStatus] = useState('');
  useDocumentSelectionPersistence({
    ready: true, selectedDocumentIds, setSelectedDocumentIds, setStatus,
  });
  return { selectedDocumentIds, setSelectedDocumentIds, status };
}

describe('useDocumentSelectionPersistence', () => {
  beforeEach(() => { vi.resetAllMocks(); });

  it('does not overwrite a newer user selection with an older normalized IPC response', async () => {
    let finishOlder!: (ids: string[]) => void;
    const firstResponse = new Promise<string[]>((resolve) => { finishOlder = resolve; });
    const rpc = vi.mocked(setDocumentSelection)
      .mockImplementationOnce(async () => firstResponse)
      .mockImplementation(async (ids) => ids);
    const { result } = renderHook(() => useSelection());

    act(() => { result.current.setSelectedDocumentIds(['outdated']); });
    await waitFor(() => expect(rpc).toHaveBeenCalledTimes(1));
    act(() => { result.current.setSelectedDocumentIds(['current']); });
    await act(async () => { finishOlder([]); await firstResponse; });
    await waitFor(() => expect(rpc).toHaveBeenCalledTimes(2));
    expect(rpc).toHaveBeenNthCalledWith(2, ['current']);
    expect(result.current.selectedDocumentIds).toEqual(['current']);
  });

  it('still applies canonical backend normalization if the user has not changed selection', async () => {
    const rpc = vi.mocked(setDocumentSelection).mockResolvedValue(['known']);
    const { result } = renderHook(() => useSelection());
    act(() => { result.current.setSelectedDocumentIds(['known', 'removed']); });
    await waitFor(() => expect(result.current.selectedDocumentIds).toEqual(['known']));
    expect(rpc).toHaveBeenCalledWith(['known', 'removed']);
    expect(result.current.status).toBe('');
  });
});
