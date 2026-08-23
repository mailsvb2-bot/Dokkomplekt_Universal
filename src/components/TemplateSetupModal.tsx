import { useEffect, useMemo, useState } from 'react';
import type { ChangeEvent, DragEvent } from 'react';
import type { DomainKind, PopupFieldConfig, WorkspaceProfileInference, WorkspaceWorkflowShape } from '../lib/types';
import { PopupFieldEditor, ensurePopupField } from './PopupFieldEditor';

interface PendingTemplateView {
  document_id: string;
  file_name: string;
  button_label: string;
  extracted_text: string;
  popup_fields: PopupFieldConfig[];
  popup_fields_edited?: boolean;
  domain_override?: DomainKind | null;
}

interface TemplateSetupModalProps {
  templateText: string;
  buttonLabel: string;
  previewTitle: string;
  pendingTemplates: PendingTemplateView[];
  draftPopupFields: PopupFieldConfig[];
  draftDomainOverride?: DomainKind | null;
  autoInferStaticTemplates?: boolean;
  workspaceInference?: WorkspaceProfileInference | null;
  workspaceShape?: WorkspaceWorkflowShape | null;
  onTemplateTextChange(value: string): void;
  onButtonLabelChange(value: string): void;
  onDraftPopupFieldsChange(fields: PopupFieldConfig[]): void;
  onDraftDomainOverrideChange?(value: DomainKind | null): void;
  onAutoInferStaticTemplatesChange?(value: boolean): void;
  onPendingTemplateLabelChange(documentId: string, value: string): void;
  onPendingTemplateDomainChange?(documentId: string, value: DomainKind | null): void;
  onApplyWorkspaceDomain?(value: DomainKind): void;
  onPendingPopupFieldsChange(documentId: string, fields: PopupFieldConfig[]): void;
  onMarkupPendingTemplate(documentId: string, selectedText: string, fieldId: string, action: 'replace' | 'insert_after'): Promise<void>;
  onLearnPendingTemplate(documentId: string, files: File[]): Promise<void>;
  onStartGuidedPendingScanner(documentId: string): void;
  onAnalyze(): void;
  onPickFile(event: ChangeEvent<HTMLInputElement>): void;
  onDropFiles(files: File[]): void;
  onCancel(): void;
  onConfirm(): void;
}

export function TemplateSetupModal(props: TemplateSetupModalProps) {
  const hasBatch = props.pendingTemplates.length > 0;
  const [scannerField, setScannerField] = useState('');
  const [selection, setSelection] = useState<{ start: number; end: number; text: string } | null>(null);
  const [activePendingId, setActivePendingId] = useState('');
  const [marking, setMarking] = useState(false);
  const activePending = useMemo(
    () => props.pendingTemplates.find((item) => item.document_id === activePendingId) ?? props.pendingTemplates[0] ?? null,
    [activePendingId, props.pendingTemplates],
  );
  const invalidLabel = props.pendingTemplates.find((item) => !item.button_label.trim());
  const invalidDomain = props.pendingTemplates.find((item) => hasInvalidCustomDomain(item.domain_override));
  const batchReady = hasBatch && !invalidLabel && !invalidDomain;
  const manualReady = Boolean(props.templateText.trim()) && !hasInvalidCustomDomain(props.draftDomainOverride);
  const confirmLabel = hasBatch ? `Создать кнопки (${props.pendingTemplates.length})` : 'Создать кнопку';

  useEffect(() => {
    if (!props.pendingTemplates.length) {
      setActivePendingId('');
      return;
    }
    if (!props.pendingTemplates.some((item) => item.document_id === activePendingId)) {
      setActivePendingId(props.pendingTemplates[0].document_id);
    }
  }, [activePendingId, props.pendingTemplates]);

  useEffect(() => {
    for (const item of props.pendingTemplates) {
      const normalized = normalizeTemplateButtonLabel(item.button_label);
      if (normalized !== item.button_label) props.onPendingTemplateLabelChange(item.document_id, normalized);
    }
  }, [props.pendingTemplates, props.onPendingTemplateLabelChange]);

  function rememberScannerQuestion(documentId: string | null, fieldId: string) {
    if (!fieldId.trim()) return;
    if (documentId) {
      const current = props.pendingTemplates.find((item) => item.document_id === documentId);
      if (current) props.onPendingPopupFieldsChange(documentId, ensurePopupField(current.popup_fields, fieldId));
    } else {
      props.onDraftPopupFieldsChange(ensurePopupField(props.draftPopupFields, fieldId));
    }
  }

  function applyVisualMarkup(action: 'replace' | 'insert_after') {
    if (!selection || !scannerField.trim()) return;
    const fieldId = scannerField.trim();
    const placeholder = `{{${fieldId}}}`;
    const replacement = action === 'replace' ? placeholder : `${selection.text}${placeholder}`;
    props.onTemplateTextChange(props.templateText.slice(0, selection.start) + replacement + props.templateText.slice(selection.end));
    rememberScannerQuestion(null, fieldId);
    setSelection(null);
  }

  async function applyPendingVisualMarkup(action: 'replace' | 'insert_after') {
    if (!activePending || !selection || !scannerField.trim()) return;
    const fieldId = scannerField.trim();
    setMarking(true);
    try {
      await props.onMarkupPendingTemplate(activePending.document_id, selection.text, fieldId, action);
      rememberScannerQuestion(activePending.document_id, fieldId);
      setSelection(null);
    } finally {
      setMarking(false);
    }
  }

  return (
    <div className="backdrop" role="dialog" aria-modal="true" aria-label="Добавление шаблонов">
      <div
        className="modal fileDropZone templateSetupWide"
        onDragOver={(event: DragEvent<HTMLDivElement>) => event.preventDefault()}
        onDrop={(event: DragEvent<HTMLDivElement>) => {
          event.preventDefault();
          const files = Array.from(event.dataTransfer.files ?? []);
          if (files.length) props.onDropFiles(files);
        }}
      >
        <h2>Создать свои кнопки</h2>
        <p className="hint">Выберите рабочие шаблоны Word. Каждый DOCX или DOCM сразу станет отдельной кнопкой.</p>
        <p className="hint">Сначала создайте кнопки и начните работать. Автоматические поля, вопросы и разметку можно добавить позже для каждой кнопки.</p>

        <label className="templateInferenceOption">
          <input
            type="checkbox"
            checked={Boolean(props.autoInferStaticTemplates)}
            onChange={(event) => props.onAutoInferStaticTemplatesChange?.(event.target.checked)}
          />
          <span>
            <strong>Безопасно попробовать авторазметку старых шаблонов</strong>
            <small>Необязательно. Только однозначные подписи и уникальные пустые зоны; исходные Word-файлы не изменяются.</small>
          </span>
        </label>

        {!hasBatch ? (
          <div className="emptyPackage templateFirstStep">
            <div><i className="ti ti-file-upload" /></div>
            <h3>Выберите шаблоны документов</h3>
            <p>Можно выбрать сразу несколько файлов. Название документа программа предложит как название кнопки.</p>
            <label className="primaryBtn fileBtn largeAction">
              Выбрать DOCX/DOCM
              <input type="file" accept=".docx,.docm" multiple onChange={props.onPickFile} data-testid="template-file-input" style={{ display: 'none' }} />
            </label>
            <details className="manualScannerDetails">
              <summary>Создать одну кнопку из вставленного текста</summary>
              <textarea
                value={props.templateText}
                onChange={(event) => props.onTemplateTextChange(event.target.value)}
                onSelect={(event) => {
                  const target = event.currentTarget;
                  const start = target.selectionStart ?? 0;
                  const end = target.selectionEnd ?? start;
                  setSelection(end > start ? { start, end, text: target.value.slice(start, end) } : null);
                }}
                spellCheck={false}
                placeholder="Вставьте текст документа"
              />
              <table className="confirm"><tbody>
                <tr><th>Документ</th><td>{props.previewTitle}</td></tr>
                <tr><th>Название кнопки</th><td><input value={props.buttonLabel} placeholder={props.previewTitle} onChange={(event) => props.onButtonLabelChange(event.target.value)} /></td></tr>
                <tr><th>Профиль</th><td><DomainOverrideEditor label="Шаблон" value={props.draftDomainOverride ?? null} onChange={(value) => props.onDraftDomainOverrideChange?.(value)} /></td></tr>
              </tbody></table>
              <button className="softBtn" type="button" onClick={props.onAnalyze}>Проверить шаблон</button>
              <details className="manualScannerDetails">
                <summary>Дополнительная разметка</summary>
                <ScannerToolbar selection={selection?.text ?? ''} fieldId={scannerField} onFieldIdChange={setScannerField} onReplace={() => applyVisualMarkup('replace')} onInsert={() => applyVisualMarkup('insert_after')} />
                <PopupFieldEditor compact fields={props.draftPopupFields} onChange={props.onDraftPopupFieldsChange} />
              </details>
            </details>
          </div>
        ) : (
          <>
            {props.workspaceInference ? (
              <WorkspaceInferenceSummary
                inference={props.workspaceInference}
                onApply={(domain) => props.onApplyWorkspaceDomain?.(domain)}
              />
            ) : null}
            {props.workspaceShape?.documents.length ? <WorkspaceShapeSummary shape={props.workspaceShape} /> : null}
            <div className="templateBatch" aria-label="Подготовленные шаблоны">
              <div className="templateBatchHead">Проверьте названия кнопок</div>
              {props.pendingTemplates.map((item) => (
                <div className={activePending?.document_id === item.document_id ? 'templateBatchRow selected' : 'templateBatchRow'} key={item.document_id}>
                  <button className="templateFileSelect" type="button" title="Открыть дополнительную настройку" onClick={() => { setActivePendingId(item.document_id); setSelection(null); }}>
                    {item.file_name}
                  </button>
                  <input aria-label={`Название документа для ${item.file_name}`} value={item.button_label} onChange={(event) => props.onPendingTemplateLabelChange(item.document_id, event.target.value)} />
                </div>
              ))}
            </div>

            <div className={`readyMessage templateReadyMessage ${batchReady ? '' : 'warning'}`}>
              <i className={batchReady ? 'ti ti-circle-check' : 'ti ti-alert-triangle'} aria-hidden="true" />
              <div>
                <strong>{batchReady ? 'Кнопки готовы к созданию' : invalidDomain ? 'Укажите свою профессию / профиль' : 'Укажите название кнопки'}</strong>
                <span>{batchReady
                  ? 'Нажмите кнопку ниже. Неразмеченные шаблоны сохранят свою форму и будут доступны сразу.'
                  : invalidDomain
                    ? `Не заполнена своя профессия / профиль для ${invalidDomain.file_name}.`
                    : `Не заполнено название для ${invalidLabel?.file_name ?? 'одного шаблона'}.`}</span>
              </div>
            </div>

            {activePending ? (
              <details className="manualScannerDetails templateAdvancedSetup">
                <summary>Необязательно: настроить автоматическое заполнение</summary>
                <div className="pendingProfileCorrection">
                  <span className="hint">Если программа неверно поняла этот конкретный шаблон, профиль можно исправить вручную.</span>
                  <DomainOverrideEditor
                    label={activePending.file_name}
                    value={activePending.domain_override ?? null}
                    onChange={(value) => props.onPendingTemplateDomainChange?.(activePending.document_id, value)}
                  />
                </div>
                <div className="pendingCursorScanner">
                  <div className="guidedTemplateLaunch">
                    <div><strong>Показать место для автоматического заполнения</strong><small>Этот шаг не нужен для создания кнопки. Его можно выполнить позже.</small></div>
                    <button className="softBtn" type="button" onClick={() => props.onStartGuidedPendingScanner(activePending.document_id)}><i className="ti ti-hand-click" aria-hidden="true" /> Открыть Word и показать место</button>

                    <label className="softBtn fileBtn">
                      <i className="ti ti-school" aria-hidden="true" /> Обучить по 3–10 примерам
                      <input
                        type="file"
                        multiple
                        accept=".docx,.docm,.pdf,.txt,.csv,.xlsx,.xls,.png,.jpg,.jpeg,.tif,.tiff,.bmp,.webp"
                        onChange={(event) => {
                          const files = Array.from(event.currentTarget.files ?? []);
                          event.currentTarget.value = '';
                          if (files.length) void props.onLearnPendingTemplate(activePending.document_id, files);
                        }}
                        style={{ display: 'none' }}
                      />
                    </label>
                  </div>
                  <details className="manualScannerDetails">
                    <summary>Ручная разметка</summary>
                    <textarea
                      value={activePending.extracted_text}
                      readOnly
                      aria-label={`Текст шаблона ${activePending.file_name} для разметки`}
                      onSelect={(event) => {
                        const target = event.currentTarget;
                        const start = target.selectionStart ?? 0;
                        const end = target.selectionEnd ?? start;
                        setSelection(end > start ? { start, end, text: target.value.slice(start, end) } : null);
                      }}
                      spellCheck={false}
                    />
                    <ScannerToolbar selection={selection?.text ?? ''} fieldId={scannerField} onFieldIdChange={setScannerField} disabled={marking} onReplace={() => void applyPendingVisualMarkup('replace')} onInsert={() => void applyPendingVisualMarkup('insert_after')} />
                  </details>
                  <PopupFieldEditor compact fields={activePending.popup_fields} onChange={(fields) => props.onPendingPopupFieldsChange(activePending.document_id, fields)} />
                </div>
              </details>
            ) : null}

            <label className="softBtn fileBtn">Добавить ещё шаблоны<input type="file" accept=".docx,.docm" multiple onChange={props.onPickFile} style={{ display: 'none' }} /></label>
          </>
        )}

        <datalist id="template-scanner-field-suggestions">
          <option value="document.number" /><option value="document.date" /><option value="subject.name" /><option value="org.name" />
          <option value="amount.total" /><option value="period.start_date" /><option value="period.end_date" /><option value="related.number" />
        </datalist>

        <div className="modalActions">
          <span className="spacer" />
          <button className="softBtn" onClick={props.onCancel}>Отмена</button>
          <button className="primaryBtn" onClick={props.onConfirm} disabled={hasBatch ? !batchReady : !manualReady}>{confirmLabel}</button>
        </div>
      </div>
    </div>
  );
}

function WorkspaceShapeSummary(props: { shape: WorkspaceWorkflowShape }) {
  const commonPreview = props.shape.common_fields.slice(0, 5);
  return (
    <div className="readyMessage templateReadyMessage workspaceInferenceCard" data-testid="workspace-shape-summary">
      <i className="ti ti-route" aria-hidden="true" />
      <div>
        <strong>Программа поняла рабочий процесс</strong>
        <span>Основной объект: {props.shape.primary_object}. Документов: {props.shape.documents.length}; общих полей: {props.shape.common_fields.length}; связей: {props.shape.relations.length}.</span>
        {commonPreview.length ? <small>Общие данные: {commonPreview.map((field) => field.title).join(' · ')}</small> : null}
        <details className="manualScannerDetails workspaceShapeDetails">
          <summary>{props.shape.mixed_workflows ? 'Показать рабочие контуры' : 'Показать карту документов'}</summary>
          <div className="workspaceShapeGrid">
            {props.shape.groups.map((group) => (
              <div key={group.group_id} className="workspaceShapeGroup">
                <strong>{group.title}</strong>
                <small>{group.document_ids.length} документ(ов)</small>
              </div>
            ))}
            {props.shape.documents.slice(0, 10).map((document) => (
              <div key={document.document_id} className="workspaceShapeDocument">
                <span>{document.title}</span><small>{document.role_label}</small>
              </div>
            ))}
          </div>
        </details>
      </div>
    </div>
  );
}

function WorkspaceInferenceSummary(props: {
  inference: WorkspaceProfileInference;
  onApply(domain: DomainKind): void;
}) {
  const domain = props.inference.suggested_domain ?? null;
  const label = domainLabel(domain);
  const percent = Math.round(props.inference.confidence * 100);
  const topEvidence = props.inference.evidence.slice(0, 4);

  if (props.inference.level === 'high' && domain) {
    return (
      <div className="readyMessage templateReadyMessage workspaceInferenceCard" data-testid="workspace-inference-high">
        <i className="ti ti-sparkles" aria-hidden="true" />
        <div>
          <strong>Программа поняла рабочий профиль: {label}</strong>
          <span>Уверенность {percent}%. Профиль определён по всему набору документов и применится автоматически ко всем создаваемым кнопкам.</span>
          {topEvidence.length ? <small>Признаки: {topEvidence.map((item) => item.title).join(' · ')}</small> : null}
        </div>
      </div>
    );
  }

  if (props.inference.level === 'medium' && domain) {
    return (
      <div className="readyMessage templateReadyMessage warning workspaceInferenceCard" data-testid="workspace-inference-medium">
        <i className="ti ti-bulb" aria-hidden="true" />
        <div>
          <strong>Похоже, рабочий профиль: {label}</strong>
          <span>Уверенность {percent}%. Программа не будет навязывать профиль при такой уверенности.</span>
          <button className="softBtn" type="button" onClick={() => props.onApply(domain)}>Да, применить ко всем кнопкам</button>
        </div>
      </div>
    );
  }

  return (
    <div className="readyMessage templateReadyMessage workspaceInferenceCard" data-testid="workspace-inference-low">
      <i className="ti ti-file-search" aria-hidden="true" />
      <div>
        <strong>Профессию выбирать не нужно</strong>
        <span>По этим документам профиль пока неоднозначен. Кнопки всё равно будут созданы; программа продолжит понимать ваш рабочий процесс по реальным документам.</span>
      </div>
    </div>
  );
}

function domainLabel(domain: DomainKind | null): string {
  if (!domain) return 'не определён';
  if (typeof domain === 'object' && 'Custom' in domain) return domain.Custom || 'свой профиль';
  return ({
    Medical: 'медицина',
    Legal: 'юридическая работа',
    Hr: 'кадровая работа',
    Accounting: 'бухгалтерия',
    Education: 'образование',
    Generic: 'универсальный документооборот',
  } as Record<string, string>)[domain] ?? String(domain);
}

function hasInvalidCustomDomain(value: DomainKind | null | undefined): boolean {
  return typeof value === 'object' && value !== null && 'Custom' in value && !value.Custom.trim();
}

function DomainOverrideEditor(props: {
  label: string;
  value: DomainKind | null;
  onChange(value: DomainKind | null): void;
}) {
  const customValue = typeof props.value === 'object' && props.value !== null && 'Custom' in props.value
    ? props.value.Custom
    : null;
  const selectedValue = customValue !== null
    ? 'custom'
    : typeof props.value === 'string'
      ? props.value
      : 'auto';
  return (
    <div className="inlineInput">
      <select
        aria-label={`Профиль для ${props.label}`}
        value={selectedValue}
        onChange={(event) => {
          const value = event.target.value;
          if (value === 'auto') props.onChange(null);
          else if (value === 'custom') props.onChange({ Custom: customValue ?? '' });
          else props.onChange(value as DomainKind);
        }}
      >
        <option value="auto">Профиль: автоматически</option>
        <option value="Generic">Универсальный документооборот</option>
        <option value="Medical">Медицина</option>
        <option value="Legal">Юридическая работа</option>
        <option value="Hr">Кадровая работа</option>
        <option value="Accounting">Бухгалтерия</option>
        <option value="Education">Образование</option>
        <option value="custom">Своя профессия / профиль</option>
      </select>
      {customValue !== null ? (
        <input
          aria-label={`Своя профессия / профиль для ${props.label}`}
          value={customValue}
          placeholder="Например: архитектор"
          onChange={(event) => props.onChange({ Custom: event.target.value })}
        />
      ) : null}
    </div>
  );
}

function normalizeTemplateButtonLabel(value: string): string {
  const trimmed = value.trim();
  const normalized = trimmed.replace(/\s+(?:№|N|#)\s*$/i, '').replace(/\s*[:;]\s*$/, '').trim();
  return normalized || trimmed;
}

function ScannerToolbar(props: {
  selection: string;
  fieldId: string;
  disabled?: boolean;
  onFieldIdChange(value: string): void;
  onReplace(): void;
  onInsert(): void;
}) {
  return (
    <div className="templateScanner">
      <div className="scannerSelection"><i className="ti ti-cursor-text" aria-hidden="true" />{props.selection ? `Выделено: ${props.selection}` : 'Выделите мышкой примерное значение'}</div>
      <input value={props.fieldId} onChange={(event) => props.onFieldIdChange(event.target.value)} placeholder="поле, например document.number" aria-label="Смысловое поле сканера" list="template-scanner-field-suggestions" />
      <button className="softBtn" type="button" onClick={props.onReplace} disabled={props.disabled || !props.selection || !props.fieldId.trim()}>Заменить</button>
      <button className="softBtn" type="button" onClick={props.onInsert} disabled={props.disabled || !props.selection || !props.fieldId.trim()}>Вставить после</button>
    </div>
  );
}
