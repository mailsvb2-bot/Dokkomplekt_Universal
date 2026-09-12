import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { FolderNamingOnboarding } from './FolderNamingOnboarding';

describe('FolderNamingOnboarding', () => {
  it('requires an explicit saved rule and uses profession-neutral wording', () => {
    const onConfirm = vi.fn();
    const onPickRoot = vi.fn();
    render(<FolderNamingOnboarding currentRoot="D:/Работа/Готовые документы" currentParts={['DocumentNumber', 'DocumentDate']} onPickRoot={onPickRoot} onConfirm={onConfirm} />);
    expect(screen.getByRole('heading', { name: 'Как называть папку комплекта?' })).toBeTruthy();
    expect(screen.queryByText(/папк.*пациент/i)).toBeNull();
    expect(screen.getByText('D:/Работа/Готовые документы')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Выбрать папку на компьютере' })).toBeTruthy();
    const save = screen.getByRole('button', { name: 'Сохранить папку и правило' }) as HTMLButtonElement;
    expect(document.activeElement).toBe(save);
    expect(save.getAttribute('aria-keyshortcuts')).toBe('Enter');
    expect(save.type).toBe('submit');
    fireEvent.click(screen.getByRole('button', { name: /Человек \+ месяц/i }));
    fireEvent.click(screen.getByRole('button', { name: 'Сохранить папку и правило' }));
    expect(onConfirm).toHaveBeenCalledWith(['ShortInitials', 'PeriodStartMonthName']);
  });
  it('submits the ready default rule through Enter-compatible form semantics', () => {
    const onConfirm = vi.fn();
    render(<FolderNamingOnboarding currentRoot="D:/Ready" currentParts={['DocumentNumber', 'DocumentDate']} onPickRoot={vi.fn()} onConfirm={onConfirm} />);
    const save = screen.getByRole('button', { name: 'Сохранить папку и правило' }) as HTMLButtonElement;
    fireEvent.submit(save.form!);
    expect(onConfirm).toHaveBeenCalledWith(['DocumentNumber', 'DocumentDate']);
  });

  it('blocks an empty folder naming rule so unrelated kits cannot collide', () => {
    const onConfirm = vi.fn();
    render(<FolderNamingOnboarding currentRoot="D:/Ready" currentParts={[]} onPickRoot={vi.fn()} onConfirm={onConfirm} />);
    expect(screen.getByText('Выберите хотя бы один компонент имени')).toBeTruthy();
    expect(screen.getByText(/Пустое имя запрещено/)).toBeTruthy();
    const save = screen.getByRole('button', { name: 'Сохранить папку и правило' }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    fireEvent.click(save);
    expect(onConfirm).not.toHaveBeenCalled();
  });

});
