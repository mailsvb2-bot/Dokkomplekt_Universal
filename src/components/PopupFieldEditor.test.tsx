import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { PopupFieldEditor } from './PopupFieldEditor';

describe('PopupFieldEditor', () => {
  it('adds a safe question and reports edits', () => {
    const onChange = vi.fn();
    const { rerender } = render(<PopupFieldEditor fields={[]} onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: '+ Добавить вопрос' }));
    expect(onChange).toHaveBeenCalledOnce();
    const fields = onChange.mock.calls[0][0];
    rerender(<PopupFieldEditor fields={fields} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText('Текст вопроса 1'), { target: { value: 'Номер дела' } });
    expect(onChange).toHaveBeenCalledTimes(2);
  });
});
