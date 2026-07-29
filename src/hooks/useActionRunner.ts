import { useCallback, useRef, useState } from 'react';

export function actionErrorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  try {
    return JSON.stringify(error);
  } catch {
    return 'Неизвестная ошибка';
  }
}

export type ActionErrorFormatter = (label: string, detail: string) => string;

const defaultErrorFormatter: ActionErrorFormatter = (_label, detail) =>
  `Не удалось выполнить действие: ${detail}`;

export function labelledActionError(label: string, detail: string): string {
  return `Ошибка «${label}»: ${detail}`;
}

export function plainActionError(_label: string, detail: string): string {
  return detail;
}

export function useActionRunner(
  onStatus: (message: string) => void,
  formatError: ActionErrorFormatter = defaultErrorFormatter,
) {
  const [busy, setBusy] = useState(false);
  const pendingCount = useRef(0);

  const run = useCallback(async <T,>(label: string, action: () => Promise<T>): Promise<T | undefined> => {
    pendingCount.current += 1;
    if (pendingCount.current === 1) setBusy(true);
    try {
      return await action();
    } catch (error) {
      onStatus(formatError(label, actionErrorMessage(error)));
      return undefined;
    } finally {
      pendingCount.current = Math.max(0, pendingCount.current - 1);
      if (pendingCount.current === 0) setBusy(false);
    }
  }, [formatError, onStatus]);

  return { busy, run };
}
