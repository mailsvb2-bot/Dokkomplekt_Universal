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
  it('opens the exact published patient folder without another success popup', async () => {
    const openFolder = vi.fn().mockResolvedValue(undefined);
    await expect(openCreatedOutputFolderSilently('  D:/Ready/Иванов И.И.  ', openFolder)).resolves.toBeUndefined();
    expect(openFolder).toHaveBeenCalledWith('D:/Ready/Иванов И.И.');
  });

  it('keeps a successful creation successful when the OS shell cannot open the folder', async () => {
    const openFolder = vi.fn().mockRejectedValue(new Error('shell unavailable'));
    await expect(openCreatedOutputFolderSilently('D:/Ready/42', openFolder)).resolves.toBeUndefined();
    expect(openFolder).toHaveBeenCalledOnce();
  });

  it('does not call the shell for an empty result path', async () => {
    const openFolder = vi.fn();
    await openCreatedOutputFolderSilently('   ', openFolder);
    expect(openFolder).not.toHaveBeenCalled();
  });
});
