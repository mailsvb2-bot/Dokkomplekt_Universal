import { describe, expect, it, vi } from 'vitest';
import { OUTPUT_ROOT_KEY } from './appSupport';
import { ensureDefaultOutputRoot, outputRootNeedsBootstrap } from './outputRootBootstrap';

class MemoryStorage {
  private readonly values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

describe('default output root bootstrap', () => {
  it('bootstraps a real Desktop destination when no output root was saved', async () => {
    const storage = new MemoryStorage();
    const resolver = vi.fn(async () => 'C:\\Users\\Doctor\\Desktop\\Выписанные пациенты');

    const resolved = await ensureDefaultOutputRoot(storage, resolver);

    expect(resolved).toBe('C:\\Users\\Doctor\\Desktop\\Выписанные пациенты');
    expect(storage.getItem(OUTPUT_ROOT_KEY)).toBe(resolved);
    expect(resolver).toHaveBeenCalledTimes(1);
  });

  it('migrates the old process-relative fallback to the Desktop destination', async () => {
    const storage = new MemoryStorage();
    storage.setItem(OUTPUT_ROOT_KEY, 'output\\готовые документы');
    const resolver = vi.fn(async () => 'D:\\Desktop\\Выписанные пациенты');

    const resolved = await ensureDefaultOutputRoot(storage, resolver);

    expect(resolved).toBe('D:\\Desktop\\Выписанные пациенты');
    expect(storage.getItem(OUTPUT_ROOT_KEY)).toBe(resolved);
    expect(resolver).toHaveBeenCalledTimes(1);
  });

  it('preserves an explicit user-selected destination', async () => {
    const storage = new MemoryStorage();
    storage.setItem(OUTPUT_ROOT_KEY, 'E:\\Документы\\Пациенты');
    const resolver = vi.fn(async () => 'C:\\Users\\Doctor\\Desktop\\Выписанные пациенты');

    const resolved = await ensureDefaultOutputRoot(storage, resolver);

    expect(resolved).toBe('E:\\Документы\\Пациенты');
    expect(storage.getItem(OUTPUT_ROOT_KEY)).toBe('E:\\Документы\\Пациенты');
    expect(resolver).not.toHaveBeenCalled();
  });

  it('physically ensures an already-saved destination without replacing the user choice', async () => {
    const storage = new MemoryStorage();
    storage.setItem(OUTPUT_ROOT_KEY, 'C:\\Users\\Doctor\\Desktop\\Выписанные пациенты');
    const resolver = vi.fn(async () => 'D:\\Other\\Выписанные пациенты');
    const ensurer = vi.fn(async (path: string) => path);

    const resolved = await ensureDefaultOutputRoot(storage, resolver, ensurer);

    expect(resolved).toBe('C:\\Users\\Doctor\\Desktop\\Выписанные пациенты');
    expect(ensurer).toHaveBeenCalledWith('C:\\Users\\Doctor\\Desktop\\Выписанные пациенты');
    expect(resolver).not.toHaveBeenCalled();
    expect(storage.getItem(OUTPUT_ROOT_KEY)).toBe(resolved);
  });

  it('ensures the new Desktop destination before saving it', async () => {
    const storage = new MemoryStorage();
    const resolver = vi.fn(async () => 'C:\\Users\\Doctor\\Desktop\\Выписанные пациенты');
    const ensurer = vi.fn(async (path: string) => path);

    const resolved = await ensureDefaultOutputRoot(storage, resolver, ensurer);

    expect(ensurer).toHaveBeenCalledWith('C:\\Users\\Doctor\\Desktop\\Выписанные пациенты');
    expect(storage.getItem(OUTPUT_ROOT_KEY)).toBe(resolved);
  });

  it('falls back safely to the existing folder picker when Desktop resolution fails', async () => {
    const storage = new MemoryStorage();
    const resolver = vi.fn(async () => { throw new Error('desktop unavailable'); });

    await expect(ensureDefaultOutputRoot(storage, resolver)).resolves.toBeNull();
    expect(storage.getItem(OUTPUT_ROOT_KEY)).toBeNull();
  });

  it('recognizes empty and legacy roots but not an explicit path', () => {
    expect(outputRootNeedsBootstrap(null)).toBe(true);
    expect(outputRootNeedsBootstrap('   ')).toBe(true);
    expect(outputRootNeedsBootstrap('output/готовые документы/')).toBe(true);
    expect(outputRootNeedsBootstrap('C:\\Users\\Doctor\\Desktop\\Выписанные пациенты')).toBe(false);
  });
});
