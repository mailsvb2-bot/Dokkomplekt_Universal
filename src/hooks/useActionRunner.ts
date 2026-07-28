import { useCallback, useState } from 'react';

function message(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  try {
    return JSON.stringify(error);
  } catch {
    return 'Неизвестная ошибка';
  }
}

export function useActionRunner(onStatus: (message: string) => void) {
  const [busy, setBusy] = useState(false);

  const run = useCallback(async <T,>(_label: string, action: () => Promise<T>): Promise<T | undefined> => {
    setBusy(true);
    try {
      return await action();
    } catch (error) {
      onStatus(`Не удалось выполнить действие: ${message(error)}`);
      return undefined;
    } finally {
      setBusy(false);
    }
  }, [onStatus]);

  return { busy, run };
}
