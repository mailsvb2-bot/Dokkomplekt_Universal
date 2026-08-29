import { describe, expect, it, vi } from 'vitest';
import { chooseExistingOutputPolicyFlow, openCreatedOutputFolderSilently } from './outputFlow';

const parts = ['DocumentNumber'] as const;

describe('chooseExistingOutputPolicyFlow', () => {
  it('fails closed before touching the filesystem when no real output folder is selected', async () => {
    const onStatus = vi.fn();
    const onMissingRoot = vi.fn();
    const getPlan = vi.fn();
    const result = await chooseExistingOutputPolicyFlow({
      outputRoot: '   ',
      folderParts: [...parts],
      labels: ['Выписка'],
      getPlan,
      confirm: vi.fn(),
      openFolder: vi.fn(),
      onStatus,
      onMissingRoot,
    });
    expect(result).toBeNull();
    expect(getPlan).not.toHaveBeenCalled();
    expect(onMissingRoot).toHaveBeenCalledOnce();
    expect(onStatus).toHaveBeenCalledWith('Сначала выберите папку готовых документов. Ничего не создано.');
  });

  it('uses the exact selected folder and creates a new version when no prior output exists', async () => {
    const getPlan = vi.fn().mockResolvedValue({ exists: false, patient_folder: 'D:/Ready/123' });
    const result = await chooseExistingOutputPolicyFlow({
      outputRoot: '  D:/Ready  ',
      folderParts: [...parts],
      labels: ['Дневники'],
      getPlan,
      confirm: vi.fn(),
      openFolder: vi.fn(),
      onStatus: vi.fn(),
      onMissingRoot: vi.fn(),
    });
    expect(result).toBe('version');
    expect(getPlan).toHaveBeenCalledWith('D:/Ready', ['DocumentNumber'], ['Дневники']);
  });

  it('does not claim that an existing folder opened when the shell failed', async () => {
    const onStatus = vi.fn();
    const result = await chooseExistingOutputPolicyFlow({
      outputRoot: 'C:/Ready',
      folderParts: [...parts],
      labels: ['Дневники'],
      getPlan: vi.fn().mockResolvedValue({ exists: true, patient_folder: 'C:/Ready/42' }),
      confirm: vi.fn().mockResolvedValue(true),
      openFolder: vi.fn().mockRejectedValue(new Error('shell unavailable')),
      onStatus,
      onMissingRoot: vi.fn(),
    });
    expect(result).toBeNull();
    expect(onStatus).toHaveBeenCalledWith(expect.stringContaining('не удалось открыть'));
  });

  it('treats a handled runner cancellation as no output action', async () => {
    const confirm = vi.fn();
    const result = await chooseExistingOutputPolicyFlow({
      outputRoot: 'C:/Ready',
      folderParts: [...parts],
      labels: ['Дневники'],
      getPlan: vi.fn().mockResolvedValue(undefined),
      confirm,
      openFolder: vi.fn(),
      onStatus: vi.fn(),
      onMissingRoot: vi.fn(),
    });
    expect(result).toBeNull();
    expect(confirm).not.toHaveBeenCalled();
  });
});

describe('openCreatedOutputFolderSilently', () => {
  it('opens the exact published patient folder and reports success', async () => {
    const openFolder = vi.fn().mockResolvedValue(undefined);
    await expect(openCreatedOutputFolderSilently('  D:/Ready/Иванов И.И.  ', openFolder)).resolves.toEqual({ opened: true });
    expect(openFolder).toHaveBeenCalledWith('D:/Ready/Иванов И.И.');
  });

  it('keeps publication successful but reports a shell-open failure to the caller', async () => {
    const openFolder = vi.fn().mockRejectedValue(new Error('shell unavailable'));
    await expect(openCreatedOutputFolderSilently('D:/Ready/42', openFolder)).resolves.toEqual({ opened: false, error: 'shell unavailable' });
    expect(openFolder).toHaveBeenCalledOnce();
  });

  it('does not call the shell for an empty result path and reports the missing path', async () => {
    const openFolder = vi.fn();
    await expect(openCreatedOutputFolderSilently('   ', openFolder)).resolves.toEqual({ opened: false, error: 'путь готового комплекта пуст' });
    expect(openFolder).not.toHaveBeenCalled();
  });
});
