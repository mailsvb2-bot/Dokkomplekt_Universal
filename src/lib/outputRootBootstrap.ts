import { desktopDir, join } from '@tauri-apps/api/path';
import { OUTPUT_ROOT_KEY } from './appSupport';

export const DEFAULT_OUTPUT_FOLDER_NAME = 'Выписанные пациенты';
const LEGACY_RELATIVE_OUTPUT_ROOT = 'output/готовые документы';

type OutputRootStorage = Pick<Storage, 'getItem' | 'setItem'>;
type OutputRootResolver = () => Promise<string>;
type OutputRootEnsurer = (path: string) => Promise<string>;

let lastBootstrapError: string | null = null;

function normalizedRoot(value: string): string {
  return value.trim().replace(/\\/g, '/').replace(/\/+$/, '').toLocaleLowerCase('ru-RU');
}

function errorDetail(error: unknown): string {
  if (error instanceof Error && error.message.trim()) return error.message.trim();
  const detail = String(error ?? '').trim();
  return detail || 'неизвестная ошибка файловой системы';
}

export function getOutputRootBootstrapError(): string | null {
  return lastBootstrapError;
}

export function outputRootNeedsBootstrap(value: string | null): boolean {
  if (!value?.trim()) return true;
  return normalizedRoot(value) === LEGACY_RELATIVE_OUTPUT_ROOT;
}

async function resolveDesktopOutputRoot(): Promise<string> {
  return join(await desktopDir(), DEFAULT_OUTPUT_FOLDER_NAME);
}

/**
 * Resolve the first-run output destination before React reads its synchronous
 * localStorage-backed state. Existing explicit user choices are preserved.
 *
 * The destination is physically ensured before React starts. That keeps the
 * visible Desktop folder contract true even before the first successful batch,
 * and also repairs a previously saved path whose directory was removed.
 * Filesystem failures are retained for the startup UI instead of being silently
 * mistaken for a usable output destination.
 */
export async function ensureDefaultOutputRoot(
  storage: OutputRootStorage = localStorage,
  resolveRoot: OutputRootResolver = resolveDesktopOutputRoot,
  ensureRoot?: OutputRootEnsurer,
): Promise<string | null> {
  const existing = storage.getItem(OUTPUT_ROOT_KEY);
  lastBootstrapError = null;

  try {
    if (!outputRootNeedsBootstrap(existing)) {
      const saved = existing!.trim();
      return ensureRoot ? (await ensureRoot(saved)).trim() || saved : saved;
    }

    const resolved = (await resolveRoot()).trim();
    if (!resolved) {
      lastBootstrapError = 'Windows не вернул путь к рабочему столу.';
      return null;
    }
    const ensured = ensureRoot ? (await ensureRoot(resolved)).trim() || resolved : resolved;
    storage.setItem(OUTPUT_ROOT_KEY, ensured);
    return ensured;
  } catch (error) {
    lastBootstrapError = `Не удалось подготовить папку готовых документов: ${errorDetail(error)}`;
    return null;
  }
}
