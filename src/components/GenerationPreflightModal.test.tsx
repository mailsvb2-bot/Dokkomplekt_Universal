import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
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
