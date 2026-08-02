import type { PromptSpec, WorkflowPlan } from '../lib/types';

interface RuntimePromptModalProps {
  title: string;
  plan: WorkflowPlan;
  answers: Record<string, string>;
  message?: string;
  busy: boolean;
  onAnswer(fieldId: string, value: string): void;
  onCancel(): void;
  onSubmit(): void;
}

export function RuntimePromptModal(props: RuntimePromptModalProps) {
  const sections = groupPrompts(props.plan.prompts);
  return (
    <div className="backdrop runtimePromptBackdrop" role="dialog" aria-modal="true" aria-label={props.title}>
      <div className="modal runtimePromptModal">
        <h2>{props.title}</h2>
        <p className="hint">
          Проверьте найденные значения и заполните недостающее. Неверное обязательное значение не закроет окно. Подтверждённые данные будут повторно использованы в остальных документах комплекта.
        </p>
        {props.plan.blocked && <div className="promptError">{props.plan.block_reasons.join('; ')}</div>}
        {props.message && <div className="promptError">{props.message}</div>}
        <div className="runtimePromptScroll">
          {sections.map(([section, prompts]) => (
            <fieldset className="runtimePromptSection" key={section}>
              <legend>{section}</legend>
              {prompts.map((prompt) => (
                <PromptInput
                  key={prompt.field_id}
                  prompt={prompt}
                  value={props.answers[prompt.field_id] ?? prompt.current_value ?? ''}
                  onChange={(value) => props.onAnswer(prompt.field_id, value)}
                />
              ))}
            </fieldset>
          ))}
        </div>
        <div className="modalActions">
          <span className="promptRequiredLegend">* обязательное поле</span>
          <span className="spacer" />
          <button className="softBtn" onClick={props.onCancel} disabled={props.busy}>Отмена</button>
          <button className="primaryBtn" onClick={props.onSubmit} disabled={props.busy || props.plan.blocked}>
            {props.busy ? 'Проверка…' : 'Применить и создать'}
          </button>
        </div>
      </div>
    </div>
  );
}

function PromptInput({ prompt, value, onChange }: { prompt: PromptSpec; value: string; onChange(value: string): void }) {
  const kind = prompt.input_kind ?? 'text';
  const inputId = `prompt-${prompt.field_id.replace(/[^a-zA-Z0-9_-]/g, '-')}`;
  const hint = prompt.validation_hint || prompt.help_text;
  return (
    <label className="runtimePromptRow" htmlFor={inputId}>
      <span className="runtimePromptLabel">
        {prompt.title}{prompt.required ? <b aria-label="обязательно"> *</b> : null}
      </span>
      <span className="runtimePromptControl">
        {kind === 'long_text' ? (
          <textarea id={inputId} value={value} onChange={(event) => onChange(event.target.value)} rows={4} />
        ) : kind === 'yes_no' ? (
          <select id={inputId} value={value} onChange={(event) => onChange(event.target.value)}>
            <option value="">Выберите…</option><option value="Нет">Нет</option><option value="Да">Да</option>
          </select>
        ) : kind === 'select' ? (
          prompt.allow_custom_option ? (
            <input id={inputId} value={value} onChange={(event) => onChange(event.target.value)} list={`${inputId}-options`} />
          ) : (
            <select id={inputId} value={value} onChange={(event) => onChange(event.target.value)}>
              <option value="">Выберите…</option>
              {(prompt.options ?? []).map((option) => <option key={option} value={option}>{option}</option>)}
            </select>
          )
        ) : (
          <input
            id={inputId}
            type={kind === 'number' || kind === 'money' ? 'text' : 'text'}
            inputMode={kind === 'number' || kind === 'money' ? 'decimal' : undefined}
            value={value}
            onChange={(event) => onChange(event.target.value)}
            placeholder={kind === 'date' ? 'ДД.ММ.ГГГГ' : undefined}
          />
        )}
        {kind === 'select' && prompt.allow_custom_option && (
          <datalist id={`${inputId}-options`}>{(prompt.options ?? []).map((option) => <option key={option} value={option} />)}</datalist>
        )}
        {hint ? <small className="runtimePromptHint">{hint}</small> : null}
      </span>
    </label>
  );
}

function groupPrompts(prompts: PromptSpec[]): Array<[string, PromptSpec[]]> {
  const groups = new Map<string, PromptSpec[]>();
  for (const prompt of prompts) {
    const section = prompt.section?.trim() || 'Данные документа';
    const list = groups.get(section) ?? [];
    list.push(prompt);
    groups.set(section, list);
  }
  return Array.from(groups.entries()).map(([section, list]) => [section, [...list].sort((a, b) => (a.order ?? 500) - (b.order ?? 500))]);
}
