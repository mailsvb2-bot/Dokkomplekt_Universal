import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { __resetInvokeForTests, __setInvokeForTests } from '../lib/api';
import { OUTPUT_NAMING_CONFIRMED_KEY, OUTPUT_PREFS_KEY, OUTPUT_ROOT_KEY } from '../lib/appSupport';
import { useOutputDestination } from './useOutputDestination';

type Call = { command: string; payload?: Record<string, unknown> };

function runAction<T>(_: string, action: () => Promise<T>, onError?: (detail: string) => void): Promise<T | undefined> {
  return action().catch((error) => {
    onError?.(error instanceof Error ? error.message : String(error));
    return undefined;
  });
}

describe('useOutputDestination durable output/watcher contract', () => {
  beforeEach(() => localStorage.clear());
  afterEach(() => { localStorage.clear(); __resetInvokeForTests(); vi.restoreAllMocks(); });

  it('restores authoritative SQLite output preferences and the installed watcher folder after restart', async () => {
    const status = vi.fn();
    __setInvokeForTests(async (command) => {
      if (command === 'get_output_preferences') {
        return { output_root: 'D:/Ready', folder_parts: ['DocumentNumber', 'DocumentDate'], naming_confirmed: true } as never;
      }
      if (command === 'get_background_watcher_state') {
        return { platform: 'windows', installed: true, watch_folder: 'C:/Inbox', output_root: 'D:/Ready', folder_parts: ['DocumentNumber', 'DocumentDate'], auto_print: false, print_copies_by_document: {}, max_parallel_cases: 2, migration_required: false } as never;
      }
      throw new Error(`unexpected command ${command}`);
    });

    const { result } = renderHook(() => useOutputDestination(runAction, status));
    await waitFor(() => expect(result.current.outputRoot).toBe('D:/Ready'));
    await waitFor(() => expect(result.current.watchFolder).toBe('C:/Inbox'));
    expect(result.current.outputPreferencesReady).toBe(true);
    expect(result.current.folderNamingConfirmed).toBe(true);
    expect(localStorage.getItem(OUTPUT_ROOT_KEY)).toBe('D:/Ready');
  });

  it('does not report authoritative output preferences ready while SQLite hydration is pending', async () => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'C:/Stale');
    localStorage.setItem(OUTPUT_PREFS_KEY, JSON.stringify(['DocumentNumber', 'DocumentDate']));
    localStorage.setItem(OUTPUT_NAMING_CONFIRMED_KEY, 'true');
    const status = vi.fn();
    let resolvePreferences!: (value: unknown) => void;
    const preferences = new Promise((resolve) => { resolvePreferences = resolve; });
    __setInvokeForTests(async (command) => {
      if (command === 'get_output_preferences') return await preferences as never;
      if (command === 'get_background_watcher_state') return { platform: 'windows', installed: true, watch_folder: 'C:/Inbox', output_root: 'D:/Ready', folder_parts: ['DocumentNumber', 'DocumentDate'], auto_print: false, print_copies_by_document: {}, max_parallel_cases: 2, migration_required: false } as never;
      throw new Error(`unexpected command ${command}`);
    });

    const { result } = renderHook(() => useOutputDestination(runAction, status));
    expect(result.current.outputRoot).toBe('C:/Stale');
    expect(result.current.outputPreferencesReady).toBe(false);
    await act(async () => {
      resolvePreferences({ output_root: 'D:/Ready', folder_parts: ['DocumentNumber', 'DocumentDate'], naming_confirmed: true });
      await preferences;
    });
    await waitFor(() => expect(result.current.outputPreferencesReady).toBe(true));
    expect(result.current.outputRoot).toBe('D:/Ready');
  });

  it('migrates a confirmed legacy user root over an unconfirmed native startup default', async () => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'E:/Doctor/Patients');
    localStorage.setItem(OUTPUT_PREFS_KEY, JSON.stringify(['DocumentNumber', 'DocumentDate']));
    localStorage.setItem(OUTPUT_NAMING_CONFIRMED_KEY, 'true');
    const calls: Call[] = [];
    const status = vi.fn();
    __setInvokeForTests(async (command, payload) => {
      calls.push({ command, payload });
      if (command === 'get_output_preferences') {
        return { output_root: 'C:/Users/Doctor/Desktop/Выписанные пациенты', folder_parts: ['DocumentNumber', 'DocumentDate'], naming_confirmed: false } as never;
      }
      if (command === 'save_output_preferences') {
        return { output_root: 'E:/Doctor/Patients', folder_parts: ['DocumentNumber', 'DocumentDate'], naming_confirmed: true } as never;
      }
      if (command === 'get_background_watcher_state') return { platform: 'windows', installed: false, migration_required: false } as never;
      throw new Error(`unexpected command ${command}`);
    });

    const { result } = renderHook(() => useOutputDestination(runAction, status));
    await waitFor(() => expect(result.current.outputPreferencesReady).toBe(true));
    expect(result.current.outputRoot).toBe('E:/Doctor/Patients');
    expect(result.current.folderNamingConfirmed).toBe(true);
    expect(calls.find((call) => call.command === 'save_output_preferences')?.payload).toMatchObject({
      req: { output_root: 'E:/Doctor/Patients', naming_confirmed: true },
    });
  });

  it('fails closed when authoritative output preference hydration fails', async () => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'C:/Stale');
    localStorage.setItem(OUTPUT_PREFS_KEY, JSON.stringify(['DocumentNumber', 'DocumentDate']));
    localStorage.setItem(OUTPUT_NAMING_CONFIRMED_KEY, 'true');
    const status = vi.fn();
    __setInvokeForTests(async (command) => {
      if (command === 'get_output_preferences') throw new Error('sqlite unavailable');
      if (command === 'get_background_watcher_state') return { platform: 'windows', installed: true, watch_folder: 'C:/Inbox', output_root: 'D:/Ready', folder_parts: ['DocumentNumber', 'DocumentDate'], auto_print: false, print_copies_by_document: {}, max_parallel_cases: 2, migration_required: false } as never;
      throw new Error(`unexpected command ${command}`);
    });

    const { result } = renderHook(() => useOutputDestination(runAction, status));
    await waitFor(() => expect(status).toHaveBeenCalled());
    expect(result.current.outputPreferencesReady).toBe(false);
    expect(result.current.folderNamingConfirmed).toBe(false);
  });

  it('does not claim a new output folder was saved when durable persistence fails', async () => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'C:/Old');
    localStorage.setItem(OUTPUT_PREFS_KEY, JSON.stringify(['DocumentNumber', 'DocumentDate']));
    localStorage.setItem(OUTPUT_NAMING_CONFIRMED_KEY, 'true');
    const status = vi.fn();
    __setInvokeForTests(async (command, payload) => {
      if (command === 'get_output_preferences') return { output_root: 'C:/Old', folder_parts: ['DocumentNumber', 'DocumentDate'], naming_confirmed: true } as never;
      if (command === 'get_background_watcher_state') return { platform: 'windows', installed: false, migration_required: false } as never;
      if (command === 'ensure_output_root') return ((payload as { req?: { output_root?: string } })?.req?.output_root ?? '') as never;
      if (command === 'save_output_preferences') throw new Error('sqlite is read-only');
      throw new Error(`unexpected command ${command}`);
    });

    const { result } = renderHook(() => useOutputDestination(runAction, status));
    await waitFor(() => expect(result.current.outputRoot).toBe('C:/Old'));
    let saved = true;
    await act(async () => { saved = await result.current.commitOutputRoot('D:/New'); });
    expect(saved).toBe(false);
    expect(result.current.outputRoot).toBe('C:/Old');
    expect(localStorage.getItem(OUTPUT_ROOT_KEY)).toBe('C:/Old');
    expect(status.mock.calls.at(-1)?.[0]).toMatch(/настройка не сохранена/i);
  });

  it('reopens authoritative hydration after a successful save following an initial SQLite read failure', async () => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'C:/Stale');
    localStorage.setItem(OUTPUT_PREFS_KEY, JSON.stringify(['DocumentNumber', 'DocumentDate']));
    localStorage.setItem(OUTPUT_NAMING_CONFIRMED_KEY, 'true');
    let initialRead = true;
    const status = vi.fn();
    __setInvokeForTests(async (command, payload) => {
      if (command === 'get_output_preferences' && initialRead) {
        initialRead = false;
        throw new Error('sqlite temporarily unavailable');
      }
      if (command === 'get_background_watcher_state') return { platform: 'windows', installed: true, watch_folder: 'C:/Inbox', output_root: 'C:/Old', folder_parts: ['DocumentNumber'], auto_print: false, print_copies_by_document: {}, max_parallel_cases: 2, migration_required: false } as never;
      if (command === 'ensure_output_root') return ((payload as { req?: { output_root?: string } })?.req?.output_root ?? '') as never;
      if (command === 'save_output_preferences') return { output_root: 'D:/Ready', folder_parts: ['DocumentNumber', 'DocumentDate'], naming_confirmed: false } as never;
      throw new Error(`unexpected command ${command}`);
    });

    const { result } = renderHook(() => useOutputDestination(runAction, status));
    await waitFor(() => expect(result.current.outputPreferencesReady).toBe(false));
    await act(async () => { expect(await result.current.commitOutputRoot('D:/Ready')).toBe(true); });
    expect(result.current.outputPreferencesReady).toBe(true);
    expect(result.current.outputRoot).toBe('D:/Ready');
  });

  it('installs the watcher with a separate output root and the current year', async () => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'D:/Ready');
    localStorage.setItem(OUTPUT_PREFS_KEY, JSON.stringify(['DocumentNumber', 'DocumentDate']));
    localStorage.setItem(OUTPUT_NAMING_CONFIRMED_KEY, 'true');
    const calls: Call[] = [];
    const status = vi.fn();
    __setInvokeForTests(async (command, payload) => {
      calls.push({ command, payload });
      if (command === 'get_output_preferences') return { output_root: 'D:/Ready', folder_parts: ['DocumentNumber', 'DocumentDate'], naming_confirmed: true } as never;
      if (command === 'get_background_watcher_state') return { platform: 'windows', installed: false, migration_required: false } as never;
      if (command === 'pick_folder') return { selected_path: 'C:/Inbox' } as never;
      if (command === 'install_background_watcher') return { platform: 'windows', installed: true, watch_folder: 'C:/Inbox', output_root: 'D:/Ready', folder_parts: ['DocumentNumber', 'DocumentDate'], auto_print: false, print_copies_by_document: {}, max_parallel_cases: 2, migration_required: false, warnings: [] } as never;
      throw new Error(`unexpected command ${command}`);
    });

    const { result } = renderHook(() => useOutputDestination(runAction, status));
    await waitFor(() => expect(result.current.outputRoot).toBe('D:/Ready'));
    await act(async () => { await result.current.chooseWatchFolder(); });
    const revisionBeforeInstall = result.current.watcherRefreshRevision;
    await act(async () => { await result.current.installWatcher(false, false, {}); });
    expect(result.current.watcherRefreshRevision).toBe(revisionBeforeInstall + 1);
    const install = calls.find((call) => call.command === 'install_background_watcher');
    expect(install?.payload).toMatchObject({
      req: {
        watch_folder: 'C:/Inbox',
        output_root: 'D:/Ready',
        folder_parts: ['DocumentNumber', 'DocumentDate'],
        default_year: new Date().getFullYear(),
      },
    });
  });
});
