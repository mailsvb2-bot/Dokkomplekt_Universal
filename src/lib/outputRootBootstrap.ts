import { desktopDir, join } from '@tauri-apps/api/path';
import { OUTPUT_ROOT_KEY } from './appSupport';

export const DEFAULT_OUTPUT_FOLDER_NAME = 'Выписанные пациенты';
const LEGACY_RELATIVE_OUTPUT_ROOT = 'output/готовые документы';

type OutputRootStorage = Pick<Storage, 'getItem' | 'setItem'>;
type OutputRootResolver = () => Promise<string>;

function normalizedRoot(value: string): string {
  return value.trim().replace(/\\/g, '/').replace(/\/+$/, '').toLocaleLowerCase('ru-RU');
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
 * The directory itself is created by the canonical Rust batch renderer on the
 * first publication via create_dir_all; this function only supplies the real,
 * user-visible absolute Desktop path instead of a process-relative fallback.
 */
export async function ensureDefaultOutputRoot(
  storage: OutputRootStorage = localStorage,
  resolveRoot: OutputRootResolver = resolveDesktopOutputRoot,
): Promise<string | null> {
  const existing = storage.getItem(OUTPUT_ROOT_KEY);
  if (!outputRootNeedsBootstrap(existing)) return existing!.trim();

  try {
    const resolved = (await resolveRoot()).trim();
    if (!resolved) return null;
    storage.setItem(OUTPUT_ROOT_KEY, resolved);
    return resolved;
  } catch {
    // Keep startup fail-safe: if the OS Desktop cannot be resolved, App falls
    // back to the existing explicit folder picker instead of guessing a path.
    return null;
  }
}
