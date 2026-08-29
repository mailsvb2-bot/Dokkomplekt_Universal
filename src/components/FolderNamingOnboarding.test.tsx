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
    fireEvent.click(screen.getByRole('button', { name: /Человек \+ месяц/i }));
    fireEvent.click(screen.getByRole('button', { name: 'Сохранить папку и правило' }));
    expect(onConfirm).toHaveBeenCalledWith(['ShortInitials', 'PeriodStartMonthName']);
  });
  it('preserves an intentionally empty folder naming rule', () => {
    const onConfirm = vi.fn();
    render(<FolderNamingOnboarding currentRoot="D:/Ready" currentParts={[]} onPickRoot={vi.fn()} onConfirm={onConfirm} />);
    expect(screen.getByText('Созданные документы')).toBeTruthy();
    expect(screen.getByText(/Ни один компонент не выбран/)).toBeTruthy();
    const save = screen.getByRole('button', { name: 'Сохранить папку и правило' }) as HTMLButtonElement;
    expect(save.disabled).toBe(false);
    fireEvent.click(save);
    expect(onConfirm).toHaveBeenCalledWith([]);
  });

});
