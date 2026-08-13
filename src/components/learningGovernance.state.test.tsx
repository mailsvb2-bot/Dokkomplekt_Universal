import { afterEach, describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { __resetInvokeForTests, __setInvokeForTests } from '../lib/api';
import { AppDialogProvider } from './AppDialogProvider';
import { LearningGovernancePanel } from './LearningGovernancePanel';

describe('learning governance state', () => {
  afterEach(() => __resetInvokeForTests());

  it('drops an old result before validating a changed domain', async () => {
    __setInvokeForTests(async (command) => {
      if (command.startsWith('list_')) return [] as never;
      if (command === 'get_learned_kit_decision') {
        return { document_ids: ['x'], source: 'test', confidence: 1, auto_apply: true, reason: 'ok' } as never;
      }
      return null as never;
    });

    const view = render(<AppDialogProvider><LearningGovernancePanel documents={[]} onStatus={() => undefined} /></AppDialogProvider>);
    fireEvent.click(view.container.querySelector('summary')!);
    const inputs = view.container.querySelectorAll('input');
    fireEvent.change(inputs[0], { target: { value: 'cluster' } });
    const action = Array.from(view.container.querySelectorAll('button')).find((button) => button.textContent?.includes('Показать'))!;
    fireEvent.click(action);
    await waitFor(() => expect(screen.queryByRole('status')).not.toBeNull());

    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'Custom' } });
    fireEvent.click(action);
    await waitFor(() => expect(screen.queryByRole('status')).toBeNull());
  });
});
