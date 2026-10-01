import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { __resetInvokeForTests, __setInvokeForTests } from '../lib/api';
import { useUpdateLifecycle } from './useUpdateLifecycle';

type Call = { command: string; payload?: Record<string, unknown> };

function runAction<T>(
  _: string,
  action: () => Promise<T>,
  onError?: (detail: string) => void,
): Promise<T | undefined> {
  return action().catch((error) => {
    onError?.(error instanceof Error ? error.message : String(error));
    return undefined;
  });
}

describe('useUpdateLifecycle', () => {
  afterEach(() => {
    __resetInvokeForTests();
    vi.restoreAllMocks();
  });

  it('keeps workspace startup usable but exposes recovery inspection failures', async () => {
    const setStatus = vi.fn();
    __setInvokeForTests(async (command) => {
      if (command === 'get_update_recovery_status') {
        throw new Error('recovery marker unreadable');
      }
      throw new Error(`unexpected command ${command}`);
    });

    const { result } = renderHook(() => useUpdateLifecycle({
      workspaceStateReady: true,
      run: runAction,
      setStatus,
      confirm: vi.fn(async () => false),
    }));

    await waitFor(() => expect(setStatus).toHaveBeenCalledWith(
      expect.stringContaining('Не удалось проверить состояние восстановления обновления: recovery marker unreadable'),
    ));
    expect(result.current.checkAndApplyUpdate).toBeTypeOf('function');
  });

  it('reports an unknown recovery state instead of silently ignoring it', async () => {
    const setStatus = vi.fn();
    __setInvokeForTests(async (command) => {
      if (command === 'get_update_recovery_status') {
        return {
          schema: 'dokkomplekt.update-recovery.v1',
          status: 'future_state',
          from_version: '18.4.7',
          target_version: '18.4.8',
          package_path: 'C:/updates/18.4.8/Dokkomplekt.exe',
          package_sha256: 'a'.repeat(64),
          package_size_bytes: 123,
          backup_dir: 'C:/backup',
          created_at: '2026-10-01T10:00:00Z',
          verified_at: null,
          last_error: null,
        } as never;
      }
      throw new Error(`unexpected command ${command}`);
    });

    renderHook(() => useUpdateLifecycle({
      workspaceStateReady: true,
      run: runAction,
      setStatus,
      confirm: vi.fn(async () => false),
    }));

    await waitFor(() => expect(setStatus).toHaveBeenCalledWith(
      'Неизвестное состояние восстановления обновления «future_state». Автоматическое продолжение обновления не выполняется.',
    ));
  });

  it('applies only the exact package metadata returned by the verified update check', async () => {
    const calls: Call[] = [];
    const setStatus = vi.fn();
    const confirm = vi.fn(async () => true);
    __setInvokeForTests(async (command, payload) => {
      calls.push({ command, payload });
      if (command === 'get_update_recovery_status') return null as never;
      if (command === 'check_for_updates') {
        return {
          available: true,
          current_version: '18.4.7',
          latest_version: '18.4.8',
          platform: 'windows-x86_64',
          message: 'Доступно обновление',
          notes: 'Security update',
          verified_package_path: 'C:/AppData/verified-updates/18.4.8/Dokkomplekt-setup.exe',
          sha256: 'b'.repeat(64),
          size_bytes: 987654,
        } as never;
      }
      if (command === 'apply_verified_update') {
        return {
          prepared: true,
          target_version: '18.4.8',
          recovery_state_path: 'C:/AppData/update-recovery.json',
          backup_dir: 'C:/AppData/update-backup/18.4.8',
          message: 'Проверенный installer запущен.',
        } as never;
      }
      throw new Error(`unexpected command ${command}`);
    });

    const { result } = renderHook(() => useUpdateLifecycle({
      workspaceStateReady: true,
      run: runAction,
      setStatus,
      confirm,
    }));
    await waitFor(() => expect(calls.some((call) => call.command === 'get_update_recovery_status')).toBe(true));

    await act(async () => {
      await result.current.checkAndApplyUpdate();
    });

    expect(confirm).toHaveBeenCalledWith(expect.objectContaining({
      title: 'Установить обновление 18.4.8?',
      confirmLabel: 'Установить и перезапустить',
    }));
    expect(calls.find((call) => call.command === 'apply_verified_update')?.payload).toEqual({
      packagePath: 'C:/AppData/verified-updates/18.4.8/Dokkomplekt-setup.exe',
      targetVersion: '18.4.8',
      sha256: 'b'.repeat(64),
      sizeBytes: 987654,
    });
    expect(setStatus).toHaveBeenCalledWith('Проверенный installer запущен.');
  });

  it('fails closed before confirmation when verified package metadata is incomplete', async () => {
    const calls: Call[] = [];
    const setStatus = vi.fn();
    const confirm = vi.fn(async () => true);
    __setInvokeForTests(async (command, payload) => {
      calls.push({ command, payload });
      if (command === 'get_update_recovery_status') return null as never;
      if (command === 'check_for_updates') {
        return {
          available: true,
          current_version: '18.4.7',
          latest_version: '18.4.8',
          platform: 'windows-x86_64',
          message: 'Доступно обновление',
          notes: null,
          verified_package_path: null,
          sha256: null,
          size_bytes: null,
        } as never;
      }
      throw new Error(`unexpected command ${command}`);
    });

    const { result } = renderHook(() => useUpdateLifecycle({
      workspaceStateReady: true,
      run: runAction,
      setStatus,
      confirm,
    }));
    await waitFor(() => expect(calls.some((call) => call.command === 'get_update_recovery_status')).toBe(true));

    await act(async () => {
      await result.current.checkAndApplyUpdate();
    });

    expect(confirm).not.toHaveBeenCalled();
    expect(calls.some((call) => call.command === 'apply_verified_update')).toBe(false);
    expect(setStatus).toHaveBeenLastCalledWith(
      'Доступна версия 18.4.8, но проверенный пакет неполон. Установка не запущена.',
    );
  });
});
