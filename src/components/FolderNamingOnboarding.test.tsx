import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { FolderNamingOnboarding } from './FolderNamingOnboarding';

describe('FolderNamingOnboarding', () => {
  it('requires an explicit saved rule and uses profession-neutral wording', () => {
    const onConfirm = vi.fn();
    render(<FolderNamingOnboarding currentParts={['DocumentNumber', 'DocumentDate']} onConfirm={onConfirm} />);
    expect(screen.getByRole('heading', { name: 'Как называть папку комплекта?' })).toBeTruthy();
    expect(screen.queryByText(/папк.*пациент/i)).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /Человек \+ месяц/i }));
    fireEvent.click(screen.getByRole('button', { name: 'Сохранить правило' }));
    expect(onConfirm).toHaveBeenCalledWith(['ShortInitials', 'PeriodStartMonthName']);
  });
});
