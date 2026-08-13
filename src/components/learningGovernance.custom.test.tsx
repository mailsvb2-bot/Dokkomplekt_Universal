import { afterEach, describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { __resetInvokeForTests, __setInvokeForTests } from '../lib/api';
import { AppDialogProvider } from './AppDialogProvider';
import { LearningGovernancePanel } from './LearningGovernancePanel';

describe('LearningGovernancePanel custom profiles', () => {
  afterEach(() => __resetInvokeForTests());

  it('preserves a user-defined profession in the Rust request', async () => {
    const calls: Array<{ command: string; payload?: Record<string, unknown> }> = [];
    __setInvokeForTests(async (command, payload) => {
      calls.push({ command, payload });
      if (command === 'list_learned_scanner_rules' || command === 'list_template_approvals') return [] as never;
      if (command === 'get_learned_kit_decision') return null as never;
      return null as never;
    });

    render(<AppDialogProvider><LearningGovernancePanel documents={[]} onStatus={() => undefined} /></AppDialogProvider>);
    fireEvent.click(screen.getByText('Обучение и подтверждения'));
    fireEvent.change(screen.getByLabelText('Профиль решения'), { target: { value: 'Custom' } });
    fireEvent.change(screen.getByLabelText('Своя профессия / профиль'), { target: { value: 'architecture' } });
    fireEvent.change(screen.getByLabelText('Идентификатор кластера'), { target: { value: 'site-report' } });
    fireEvent.click(screen.getByRole('button', { name: 'Показать решение' }));

    await waitFor(() => {
      const call = calls.find((item) => item.command === 'get_learned_kit_decision');
      expect(call?.payload).toEqual({
        req: { domain: { Custom: 'architecture' }, cluster_id: 'site-report', pack_id: null },
      });
    });
  });

  it('requires a non-empty Custom profile id', async () => {
    const statuses: string[] = [];
    const commands: string[] = [];
    __setInvokeForTests(async (command) => {
      commands.push(command);
      if (command === 'list_learned_scanner_rules' || command === 'list_template_approvals') return [] as never;
      return null as never;
    });

    render(<AppDialogProvider><LearningGovernancePanel documents={[]} onStatus={(message) => statuses.push(message)} /></AppDialogProvider>);
    fireEvent.click(screen.getByText('Обучение и подтверждения'));
    fireEvent.change(screen.getByLabelText('Профиль решения'), { target: { value: 'Custom' } });
    fireEvent.change(screen.getByLabelText('Идентификатор кластера'), { target: { value: 'site-report' } });
    fireEvent.click(screen.getByRole('button', { name: 'Показать решение' }));

    await waitFor(() => expect(statuses.at(-1)).toBe('Укажите идентификатор своей профессии / профиля.'));
    expect(commands).not.toContain('get_learned_kit_decision');
  });
});
