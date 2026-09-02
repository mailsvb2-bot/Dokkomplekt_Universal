import type { PromptSpec } from './types';

export function isAffirmativeWorkflowValue(value: string): boolean {
  return ['да', 'yes', 'true'].includes(
    value.trim().toLowerCase().replaceAll('ё', 'е'),
  );
}

/**
 * Return the prompts that are active for the current answers.
 *
 * A Yes/No link is a visibility dependency: the linked field exists only when
 * its source answer is affirmative. Other links (for example date-copy links)
 * remain active and are handled by the normal workflow/popup logic.
 */
export function activeWorkflowPrompts(
  prompts: PromptSpec[],
  answers: Record<string, string>,
): PromptSpec[] {
  const byId = new Map(prompts.map((prompt) => [prompt.field_id, prompt]));
  return prompts.filter((prompt) => {
    if (!prompt.linked_to) return true;
    const source = byId.get(prompt.linked_to);
    if (source?.input_kind !== 'yes_no') return true;
    return isAffirmativeWorkflowValue(answers[source.field_id] ?? source.current_value ?? '');
  });
}

/**
 * Apply one user answer while preserving the generic linked-field contract.
 *
 * Yes/No links control visibility only: the literal "Да"/"Нет" must never
 * be copied into the linked business field. Other links (for example date-copy
 * prompts) keep the donor-compatible copy-forward behavior until the user edits
 * the linked value independently.
 */
export function updateWorkflowAnswers(
  prompts: PromptSpec[],
  previous: Record<string, string>,
  prompt: PromptSpec,
  value: string,
): Record<string, string> {
  const previousSourceValue = previous[prompt.field_id] ?? prompt.current_value ?? '';
  const next = { ...previous, [prompt.field_id]: value };
  for (const linkedPrompt of prompts) {
    if (linkedPrompt.linked_to !== prompt.field_id) continue;
    if (prompt.input_kind === 'yes_no') continue;
    const linkedCurrent = previous[linkedPrompt.field_id] ?? linkedPrompt.current_value ?? '';
    if (!linkedCurrent || linkedCurrent === previousSourceValue) next[linkedPrompt.field_id] = value;
  }
  return next;
}
