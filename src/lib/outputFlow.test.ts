import { describe, expect, it, vi } from 'vitest';
import { chooseExistingOutputPolicyFlow, prepareGenerationOutputFlow } from './outputFlow';

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

describe('prepareGenerationOutputFlow', () => {
  it('recovers an empty UI root from the backend-created canonical folder', async () => {
    const onResolvedRoot = vi.fn();
    const getPlan = vi.fn().mockResolvedValue({ exists: false, patient_folder: 'C:/Users/Test/Desktop/Выписанные пациенты/123' });
    const result = await prepareGenerationOutputFlow({
      outputRoot: '',
      folderParts: [...parts],
      labels: ['Выписка'],
      getDefaultRoot: vi.fn().mockResolvedValue(' C:/Users/Test/Desktop/Выписанные пациенты '),
      getPlan,
      confirm: vi.fn(),
      openFolder: vi.fn(),
      onStatus: vi.fn(),
      onMissingRoot: vi.fn(),
      onResolvedRoot,
    });
    expect(result).toEqual({ outputRoot: 'C:/Users/Test/Desktop/Выписанные пациенты', existingOutputPolicy: 'version' });
    expect(onResolvedRoot).toHaveBeenCalledWith('C:/Users/Test/Desktop/Выписанные пациенты');
    expect(getPlan).toHaveBeenCalledWith('C:/Users/Test/Desktop/Выписанные пациенты', ['DocumentNumber'], ['Выписка']);
  });

  it('fails closed with an actionable message when the backend cannot create the default folder', async () => {
    const onStatus = vi.fn();
    const onMissingRoot = vi.fn();
    const getPlan = vi.fn();
    const result = await prepareGenerationOutputFlow({
      outputRoot: '',
      folderParts: [...parts],
      labels: ['Выписка'],
      getDefaultRoot: vi.fn().mockResolvedValue(''),
      getPlan,
      confirm: vi.fn(),
      openFolder: vi.fn(),
      onStatus,
      onMissingRoot,
      onResolvedRoot: vi.fn(),
    });
    expect(result).toBeNull();
    expect(getPlan).not.toHaveBeenCalled();
    expect(onMissingRoot).toHaveBeenCalledOnce();
    expect(onStatus).toHaveBeenCalledWith('Не удалось создать стандартную папку «Выписанные пациенты». Выберите папку готовых документов вручную.');
  });
});
