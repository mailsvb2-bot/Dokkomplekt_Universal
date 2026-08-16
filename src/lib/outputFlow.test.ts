import { describe, expect, it, vi } from 'vitest';
import { chooseExistingOutputPolicyFlow } from './outputFlow';

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
});
