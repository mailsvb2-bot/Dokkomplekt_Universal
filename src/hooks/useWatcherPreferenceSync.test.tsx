import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { __resetInvokeForTests, __setInvokeForTests } from '../lib/api';
import { useWatcherPreferenceSync } from './useWatcherPreferenceSync';

describe('useWatcherPreferenceSync', () => {
  afterEach(() => { __resetInvokeForTests(); vi.restoreAllMocks(); });

  it('does not overwrite watcher destination before authoritative output preferences are hydrated', async () => {
    const calls: Array<{ command: string; payload?: unknown }> = [];
    __setInvokeForTests(async (command, payload) => {
      calls.push({ command, payload });
      if (command === 'get_background_watcher_state') {
        return { platform: 'windows', installed: true, watch_folder: 'C:/Inbox', output_root: 'C:/Old', folder_parts: ['DocumentNumber'], auto_print: false, print_copies_by_document: {}, max_parallel_cases: 2, migration_required: false } as never;
      }
      if (command === 'update_background_watcher_preferences') return true as never;
      throw new Error(`unexpected command ${command}`);
    });
    const setStatus = vi.fn();
    const setAutoPrint = vi.fn();
    const setPrintCopies = vi.fn();
    const { rerender } = renderHook(
      ({ ready }) => useWatcherPreferenceSync({
        outputPreferencesReady: ready,
        watcherRefreshRevision: 0,
        folderNamingConfirmed: true,
        outputRoot: 'D:/Ready',
        folderParts: ['DocumentNumber', 'DocumentDate'],
        autoPrint: false,
        printCopies: {},
        setAutoPrint,
        setPrintCopies,
        setStatus,
      }),
      { initialProps: { ready: false } },
    );

    await waitFor(() => expect(calls.some((call) => call.command === 'get_background_watcher_state')).toBe(true));
    expect(calls.some((call) => call.command === 'update_background_watcher_preferences')).toBe(false);

    rerender({ ready: true });
    await waitFor(() => expect(calls.some((call) => call.command === 'update_background_watcher_preferences')).toBe(true));
    const update = calls.find((call) => call.command === 'update_background_watcher_preferences');
    expect(update?.payload).toMatchObject({ req: { output_root: 'D:/Ready', folder_parts: ['DocumentNumber', 'DocumentDate'] } });
  });

  it('fails closed when watcher state cannot be restored', async () => {
    const calls: string[] = [];
    __setInvokeForTests(async (command) => {
      calls.push(command);
      if (command === 'get_background_watcher_state') throw new Error('watcher config unreadable');
      if (command === 'update_background_watcher_preferences') return true as never;
      throw new Error(`unexpected command ${command}`);
    });
    const setStatus = vi.fn();
    renderHook(() => useWatcherPreferenceSync({
      outputPreferencesReady: true,
      watcherRefreshRevision: 0,
      folderNamingConfirmed: true,
      outputRoot: 'D:/Ready',
      folderParts: ['DocumentNumber'],
      autoPrint: false,
      printCopies: {},
      setAutoPrint: vi.fn(),
      setPrintCopies: vi.fn(),
      setStatus,
    }));

    await waitFor(() => expect(setStatus).toHaveBeenCalledWith(expect.stringMatching(/не удалось восстановить настройки фонового агента/i)));
    expect(calls).not.toContain('update_background_watcher_preferences');
  });

  it('keeps synchronization blocked for a legacy watcher until reinstall refreshes state', async () => {
    const calls: string[] = [];
    let migrated = false;
    __setInvokeForTests(async (command) => {
      calls.push(command);
      if (command === 'get_background_watcher_state') {
        return { platform: 'windows', installed: true, watch_folder: 'C:/Inbox', output_root: 'D:/Ready', folder_parts: ['DocumentNumber'], auto_print: false, print_copies_by_document: {}, max_parallel_cases: 2, migration_required: !migrated } as never;
      }
      if (command === 'update_background_watcher_preferences') return true as never;
      throw new Error(`unexpected command ${command}`);
    });
    const base = {
      outputPreferencesReady: true,
      folderNamingConfirmed: true,
      outputRoot: 'D:/Ready',
      folderParts: ['DocumentNumber'] as const,
      autoPrint: false,
      printCopies: {},
      setAutoPrint: vi.fn(),
      setPrintCopies: vi.fn(),
      setStatus: vi.fn(),
    };
    const { rerender } = renderHook(
      ({ revision }) => useWatcherPreferenceSync({ ...base, folderParts: [...base.folderParts], watcherRefreshRevision: revision }),
      { initialProps: { revision: 0 } },
    );
    await waitFor(() => expect(calls.filter((call) => call === 'get_background_watcher_state')).toHaveLength(1));
    expect(calls).not.toContain('update_background_watcher_preferences');

    migrated = true;
    rerender({ revision: 1 });
    await waitFor(() => expect(calls.filter((call) => call === 'get_background_watcher_state')).toHaveLength(2));
    await waitFor(() => expect(calls).toContain('update_background_watcher_preferences'));
  });

  it('rehydrates watcher state after an in-session reinstall refresh signal', async () => {
    const calls: string[] = [];
    let repaired = false;
    __setInvokeForTests(async (command) => {
      calls.push(command);
      if (command === 'get_background_watcher_state') {
        if (!repaired) throw new Error('watcher config unreadable');
        return { platform: 'windows', installed: true, watch_folder: 'C:/Inbox', output_root: 'D:/Ready', folder_parts: ['DocumentNumber'], auto_print: false, print_copies_by_document: {}, max_parallel_cases: 2, migration_required: false } as never;
      }
      if (command === 'update_background_watcher_preferences') return true as never;
      throw new Error(`unexpected command ${command}`);
    });
    const common = {
      outputPreferencesReady: true,
      folderNamingConfirmed: true,
      outputRoot: 'D:/Ready',
      folderParts: ['DocumentNumber'] as const,
      autoPrint: false,
      printCopies: {},
      setAutoPrint: vi.fn(),
      setPrintCopies: vi.fn(),
      setStatus: vi.fn(),
    };
    const { rerender } = renderHook(
      ({ revision }) => useWatcherPreferenceSync({ ...common, folderParts: [...common.folderParts], watcherRefreshRevision: revision }),
      { initialProps: { revision: 0 } },
    );
    await waitFor(() => expect(common.setStatus).toHaveBeenCalledWith(expect.stringMatching(/не удалось восстановить/i)));
    expect(calls).not.toContain('update_background_watcher_preferences');

    repaired = true;
    rerender({ revision: 1 });
    await waitFor(() => expect(calls.filter((call) => call === 'get_background_watcher_state')).toHaveLength(2));
    await waitFor(() => expect(calls).toContain('update_background_watcher_preferences'));
  });
});
