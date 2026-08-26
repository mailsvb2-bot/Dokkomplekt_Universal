import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState, type Dispatch, type SetStateAction } from 'react';
import { describe, expect, it } from 'vitest';
import type { PromptSpec, WorkflowPlan } from '../lib/types';
import { GenerationPreflightModal } from './GenerationPreflightModal';

const sickLeave: PromptSpec = {
  field_id: 'medical.diary_sick_leave_epicrisis',
  title: 'Лечится по больничному листу?',
  required: true,
  input_kind: 'yes_no',
  ask_mode: 'always',
  options: ['Нет', 'Да'],
};
const correction: PromptSpec = {
  field_id: 'medical.diary_treatment_correction',
  title: 'Коррекция лечения',
  required: false,
  input_kind: 'long_text',
  ask_mode: 'always',
  linked_to: sickLeave.field_id,
  current_value: 'Лекарства принимает согласно назначениям.',
};
const plan: WorkflowPlan = {
  document_id: 'diaries',
  prompts: [sickLeave, correction],
  blocked: false,
  block_reasons: [],
};

function Harness() {
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [skippedAnswers, setSkippedAnswers] = useState<Record<string, boolean>>({});
  return (
    <GenerationPreflightModal
      plan={plan}
      documents={[]}
      selectedDocumentIds={[]}
      answers={answers}
      skippedAnswers={skippedAnswers}
      busy={false}
      loading={false}
      generationError={null}
      invalidFieldId={null}
      showSickLeaveOption={false}
      sickLeaveEnabled={false}
      setAnswers={setAnswers}
      setSkippedAnswers={setSkippedAnswers}
      onSickLeaveChange={() => undefined}
      onCancel={() => undefined}
      onConfirm={() => undefined}
    />
  );
}

describe('GenerationPreflightModal linked Yes/No visibility', () => {
  it('shows the linked correction only after an affirmative answer and never copies Да into it', () => {
    render(<Harness />);
    expect(screen.getByText('Лечится по больничному листу?')).toBeTruthy();
    expect(screen.queryByText('Коррекция лечения')).toBeNull();

    fireEvent.change(screen.getByLabelText('Лечится по больничному листу?*'), { target: { value: 'Да' } });
    expect(screen.getByText('Коррекция лечения')).toBeTruthy();
    expect((screen.getByLabelText('Коррекция лечения') as HTMLTextAreaElement).value).toBe('Лекарства принимает согласно назначениям.');

    fireEvent.change(screen.getByLabelText('Лечится по больничному листу?*'), { target: { value: 'Нет' } });
    expect(screen.queryByText('Коррекция лечения')).toBeNull();
  });
});


describe('GenerationPreflightModal validation feedback', () => {
  it('keeps a missing-field error next to Create documents and focuses the exact field', async () => {
    const requiredPrompt: PromptSpec = {
      field_id: 'medical.vk_mse_protocol_number',
      title: 'Номер протокола ВК на МСЭ',
      required: true,
      input_kind: 'text',
      ask_mode: 'always',
    };
    const requiredPlan: WorkflowPlan = {
      document_id: 'medical.vk_mse', prompts: [requiredPrompt], blocked: false, block_reasons: [],
    };
    const setAnswers = (() => undefined) as unknown as Dispatch<SetStateAction<Record<string, string>>>;
    const setSkippedAnswers = (() => undefined) as unknown as Dispatch<SetStateAction<Record<string, boolean>>>;
    render(
      <GenerationPreflightModal
        plan={requiredPlan}
        documents={[]}
        selectedDocumentIds={[]}
        answers={{}}
        skippedAnswers={{}}
        busy={false}
        loading={false}
        generationError="Не заполнено обязательное поле: Номер протокола ВК на МСЭ."
        invalidFieldId="medical.vk_mse_protocol_number"
        showSickLeaveOption={false}
        sickLeaveEnabled={false}
        setAnswers={setAnswers}
        setSkippedAnswers={setSkippedAnswers}
        onSickLeaveChange={() => undefined}
        onCancel={() => undefined}
        onConfirm={() => undefined}
      />,
    );

    expect(screen.getByTestId('generation-error').textContent).toContain('Номер протокола ВК на МСЭ');
    const field = screen.getByLabelText('Номер протокола ВК на МСЭ*');
    await waitFor(() => expect(document.activeElement).toBe(field));
    expect(screen.getByRole('button', { name: 'Создать документы' })).toBeTruthy();
  });
});
