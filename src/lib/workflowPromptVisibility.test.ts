import { describe, expect, it } from 'vitest';
import type { PromptSpec } from './types';
import { activeWorkflowPrompts, updateWorkflowAnswers } from './workflowPromptVisibility';

const yesNo: PromptSpec = {
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
  ask_mode: 'confirm',
  linked_to: yesNo.field_id,
  current_value: 'Лекарства принимает согласно назначениям.',
};
describe('workflow prompt visibility and linked answers', () => {
  it('treats a Yes/No link as visibility only and never copies the literal answer', () => {
    const prompts = [yesNo, correction];
    expect(activeWorkflowPrompts(prompts, {})).toEqual([yesNo]);

    const afterYes = updateWorkflowAnswers(prompts, {}, yesNo, 'Да');
    expect(afterYes).toEqual({ [yesNo.field_id]: 'Да' });
    expect(activeWorkflowPrompts(prompts, afterYes)).toEqual(prompts);

    const afterNo = updateWorkflowAnswers(prompts, afterYes, yesNo, 'Нет');
    expect(afterNo[correction.field_id]).toBeUndefined();
    expect(activeWorkflowPrompts(prompts, afterNo)).toEqual([yesNo]);
  });

  it('keeps copy-forward behavior for non-boolean linked prompts', () => {
    const source: PromptSpec = { field_id: 'start', title: 'Дата начала', required: true, input_kind: 'date', ask_mode: 'always' };
    const linked: PromptSpec = { field_id: 'end', title: 'Дата окончания', required: false, input_kind: 'date', ask_mode: 'always', linked_to: 'start' };
    expect(updateWorkflowAnswers([source, linked], {}, source, '02.09.2026')).toEqual({ start: '02.09.2026', end: '02.09.2026' });
  });
});
