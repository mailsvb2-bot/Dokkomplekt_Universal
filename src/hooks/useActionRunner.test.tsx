import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { labelledActionError, useActionRunner } from './useActionRunner';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe('useActionRunner', () => {
  it('stays busy until every overlapping action has completed', async () => {
    const first = deferred<string>();
    const second = deferred<string>();
    const { result } = renderHook(() => useActionRunner(vi.fn()));

    let firstRun!: Promise<string | undefined>;
    let secondRun!: Promise<string | undefined>;
    act(() => {
      firstRun = result.current.run('first', () => first.promise);
      secondRun = result.current.run('second', () => second.promise);
    });
    expect(result.current.busy).toBe(true);

    await act(async () => {
      first.resolve('one');
      await firstRun;
    });
    expect(result.current.busy).toBe(true);

    await act(async () => {
      second.resolve('two');
      await secondRun;
    });
    expect(result.current.busy).toBe(false);
  });

  it('formats failures and clears busy after rejection', async () => {
    const onStatus = vi.fn();
    const { result } = renderHook(() => useActionRunner(onStatus, labelledActionError));

    await act(async () => {
      const value = await result.current.run('печать', async () => {
        throw new Error('принтер недоступен');
      });
      expect(value).toBeUndefined();
    });

    expect(onStatus).toHaveBeenCalledWith('Ошибка «печать»: принтер недоступен');
    expect(result.current.busy).toBe(false);
  });
});
