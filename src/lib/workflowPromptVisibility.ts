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
