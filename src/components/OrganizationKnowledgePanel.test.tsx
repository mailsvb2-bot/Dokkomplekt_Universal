import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { __resetInvokeForTests, __setInvokeForTests } from '../lib/api';
import { OrganizationKnowledgePanel } from './OrganizationKnowledgePanel';

describe('OrganizationKnowledgePanel', () => {
  afterEach(() => __resetInvokeForTests());

  it('survives malformed list responses and validates field lines before saving', async () => {
    __setInvokeForTests(async (command) => {
      if (command === 'list_organization_knowledge') return null as never;
      return [] as never;
    });
    const onStatus = vi.fn();
    render(<OrganizationKnowledgePanel onStatus={onStatus} />);
    await screen.findByText('В этой категории записей пока нет.');
    fireEvent.change(screen.getByPlaceholderText('org.main'), { target: { value: 'org.main' } });
    fireEvent.change(screen.getByPlaceholderText('Основная организация'), { target: { value: 'Организация' } });
    fireEvent.change(screen.getByRole('textbox', { name: /Смысловые поля/ }), { target: { value: 'сломанная строка' } });
    fireEvent.click(screen.getByRole('button', { name: 'Сохранить запись' }));
    await waitFor(() => expect(onStatus).toHaveBeenCalledWith(expect.stringContaining('field.id=значение')));
  });
});
