import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';

export interface AppConfirmOptions {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}

export interface AppPromptOptions {
  title: string;
  message?: string;
  label: string;
  initialValue?: string;
  placeholder?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  required?: boolean;
  multiline?: boolean;
}

export interface AppFormField {
  name: string;
  label: string;
  initialValue?: string;
  placeholder?: string;
  required?: boolean;
  multiline?: boolean;
}

export interface AppFormOptions {
  title: string;
  message?: string;
  fields: AppFormField[];
  acknowledgement?: { label: string; required?: boolean };
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}

interface AppDialogApi {
  confirm(options: AppConfirmOptions): Promise<boolean>;
  prompt(options: AppPromptOptions): Promise<string | null>;
  form(options: AppFormOptions): Promise<Record<string, string> | null>;
}

type Request =
  | { id: number; kind: 'confirm'; options: AppConfirmOptions; resolve(value: boolean): void }
  | { id: number; kind: 'prompt'; options: AppPromptOptions; resolve(value: string | null): void }
  | { id: number; kind: 'form'; options: AppFormOptions; resolve(value: Record<string, string> | null): void };

const unavailableApi: AppDialogApi = {
  confirm: async () => false,
  prompt: async () => null,
  form: async () => null,
};

const AppDialogContext = createContext<AppDialogApi>(unavailableApi);

export function useAppDialog(): AppDialogApi {
  return useContext(AppDialogContext);
}

export function AppDialogProvider({ children }: { children: ReactNode }) {
  const [request, setRequest] = useState<Request | null>(null);
  const active = useRef<Request | null>(null);
  const queue = useRef<Request[]>([]);
  const [values, setValues] = useState<Record<string, string>>({});
  const [acknowledged, setAcknowledged] = useState(false);
  const nextRequestId = useRef(0);

  const activate = useCallback((next: Request | null) => {
    active.current = next;
    setRequest(next);
    setAcknowledged(false);
    if (next?.kind === 'prompt') {
      setValues({ value: next.options.initialValue ?? '' });
    } else if (next?.kind === 'form') {
      setValues(Object.fromEntries(next.options.fields.map(field => [field.name, field.initialValue ?? ''])));
    } else {
      setValues({});
    }
  }, []);

  const enqueue = useCallback((next: Request) => {
    if (active.current) queue.current.push(next);
    else activate(next);
  }, [activate]);

  const finish = useCallback((value: boolean | string | null | Record<string, string>) => {
    const current = active.current;
    if (!current) return;
    if (current.kind === 'confirm') current.resolve(Boolean(value));
    else if (current.kind === 'prompt') current.resolve(typeof value === 'string' ? value : null);
    else current.resolve(value && typeof value === 'object' ? value as Record<string, string> : null);
    activate(queue.current.shift() ?? null);
  }, [activate]);

  const api = useMemo<AppDialogApi>(() => ({
    confirm: options => new Promise(resolve => enqueue({ id: ++nextRequestId.current, kind: 'confirm', options, resolve })),
    prompt: options => new Promise(resolve => enqueue({ id: ++nextRequestId.current, kind: 'prompt', options, resolve })),
    form: options => new Promise(resolve => enqueue({ id: ++nextRequestId.current, kind: 'form', options, resolve })),
  }), [enqueue]);

  useEffect(() => {
    if (!request) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') finish(request.kind === 'confirm' ? false : null);
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [finish, request]);

  const canSubmit = request?.kind === 'prompt'
    ? (!request.options.required || Boolean(values.value?.trim()))
    : request?.kind === 'form'
      ? request.options.fields.every(field => !field.required || Boolean(values[field.name]?.trim()))
        && (!request.options.acknowledgement?.required || acknowledged)
      : true;

  const cancel = () => finish(request?.kind === 'confirm' ? false : null);
  const submit = () => {
    if (!request || !canSubmit) return;
    if (request.kind === 'confirm') finish(true);
    else if (request.kind === 'prompt') finish(values.value?.trim() ?? '');
    else finish(Object.fromEntries(Object.entries(values).map(([key, value]) => [key, value.trim()])));
  };

  return (
    <AppDialogContext.Provider value={api}>
      {children}
      {request && (
        <div key={request.id} className="backdrop appDialogBackdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) cancel(); }}>
          <div className="modal appDialog" role="dialog" aria-modal="true" aria-labelledby="app-dialog-title">
            <h2 id="app-dialog-title">{request.options.title}</h2>
            {'message' in request.options && request.options.message ? <p className="hint appDialogMessage">{request.options.message}</p> : null}

            {request.kind === 'prompt' && (
              <label className="appDialogField">
                <span>{request.options.label}{request.options.required ? ' *' : ''}</span>
                {request.options.multiline ? (
                  <textarea autoFocus value={values.value ?? ''} placeholder={request.options.placeholder} onChange={event => setValues({ value: event.target.value })} />
                ) : (
                  <input autoFocus value={values.value ?? ''} placeholder={request.options.placeholder} onChange={event => setValues({ value: event.target.value })} onKeyDown={event => { if (event.key === 'Enter') submit(); }} />
                )}
              </label>
            )}

            {request.kind === 'form' && (
              <div className="appDialogFields">
                {request.options.fields.map((field, index) => (
                  <label className="appDialogField" key={field.name}>
                    <span>{field.label}{field.required ? ' *' : ''}</span>
                    {field.multiline ? (
                      <textarea autoFocus={index === 0} value={values[field.name] ?? ''} placeholder={field.placeholder} onChange={event => setValues(current => ({ ...current, [field.name]: event.target.value }))} />
                    ) : (
                      <input autoFocus={index === 0} value={values[field.name] ?? ''} placeholder={field.placeholder} onChange={event => setValues(current => ({ ...current, [field.name]: event.target.value }))} />
                    )}
                  </label>
                ))}
                {request.options.acknowledgement && (
                  <label className="checkLine appDialogAcknowledgement">
                    <input type="checkbox" checked={acknowledged} onChange={event => setAcknowledged(event.target.checked)} />
                    <span>{request.options.acknowledgement.label}</span>
                  </label>
                )}
              </div>
            )}

            <div className="modalActions">
              <span className="spacer" />
              <button className="softBtn" type="button" onClick={cancel}>{request.options.cancelLabel ?? 'Отмена'}</button>
              <button className={'danger' in request.options && request.options.danger ? 'softBtn danger' : 'primaryBtn'} type="button" onClick={submit} disabled={!canSubmit}>
                {request.options.confirmLabel ?? 'Продолжить'}
              </button>
            </div>
          </div>
        </div>
      )}
    </AppDialogContext.Provider>
  );
}
