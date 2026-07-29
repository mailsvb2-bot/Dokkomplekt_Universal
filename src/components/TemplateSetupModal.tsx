import { useEffect, useMemo, useState } from 'react';
import type { ChangeEvent, DragEvent } from 'react';
import type { PopupFieldConfig } from '../lib/types';
import { PopupFieldEditor, ensurePopupField } from './PopupFieldEditor';

interface PendingTemplateView {
  document_id: string;
  file_name: string;
  button_label: string;
  extracted_text: string;
  popup_fields: PopupFieldConfig[];
}

interface TemplateSetupModalProps {
  templateText: string;
  buttonLabel: string;
  previewTitle: string;
  pendingTemplates: PendingTemplateView[];
  draftPopupFields: PopupFieldConfig[];
  onTemplateTextChange(value: string): void;
  onButtonLabelChange(value: string): void;
  onDraftPopupFieldsChange(fields: PopupFieldConfig[]): void;
  onPendingTemplateLabelChange(documentId: string, value: string): void;
  onPendingPopupFieldsChange(documentId: string, fields: PopupFieldConfig[]): void;
  onMarkupPendingTemplate(documentId: string, selectedText: string, fieldId: string, action: 'replace' | 'insert_after'): Promise<void>;
  onStartGuidedPendingScanner(documentId: string): void;
  onAnalyze(): void;
  onPickFile(event: ChangeEvent<HTMLInputElement>): void;
  onDropFiles(files: File[]): void;
  onCancel(): void;
  onConfirm(): void;
}

export function TemplateSetupModal(props: TemplateSetupModalProps) {
  const hasBatch = props.pendingTemplates.length > 0;
  const unmarkedTemplateCount = props.pendingTemplates.filter((item) => !hasConfirmedPlaceholder(item.extracted_text)).length;
  const batchReady = hasBatch && unmarkedTemplateCount === 0;
  const manualReady = hasConfirmedPlaceholder(props.templateText);
  const [scannerField, setScannerField] = useState('');
  const [selection, setSelection] = useState<{ start: number; end: number; text: string } | null>(null);
  const [activePendingId, setActivePendingId] = useState('');
  const [marking, setMarking] = useState(false);
  const activePending = useMemo(
    () => props.pendingTemplates.find((item) => item.document_id === activePendingId) ?? props.pendingTemplates[0] ?? null,
    [activePendingId, props.pendingTemplates],
  );

  useEffect(() => {
    if (!props.pendingTemplates.length) {
      setActivePendingId('');
      return;
    }
    if (!props.pendingTemplates.some((item) => item.document_id === activePendingId)) {
      setActivePendingId(props.pendingTemplates[0].document_id);
    }
  }, [activePendingId, props.pendingTemplates]);

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
    props.onTemplateTextChange(
      props.templateText.slice(0, selection.start) + replacement + props.templateText.slice(selection.end),
    );
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

  const confirmLabel = hasBatch
    ? `Создать кнопки (${props.pendingTemplates.length})`
    : 'Создать кнопку';

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
        <h2>Создать кнопки документов</h2>
        <p className="hint">
          Выберите свои DOCX или DOCM. Название каждого документа станет кнопкой на главном экране.
        </p>
        <p className="hint">
          Шаблон задаёт форму и расположение полей. Данные нового документа берутся из исходного файла и подтверждённых ответов, а не копируются из текста обучающего шаблона.
        </p>

        {!hasBatch ? (
          <div className="emptyPackage templateFirstStep">
            <div><i className="ti ti-file-upload" /></div>
            <h3>1. Выберите шаблоны</h3>
            <p>Можно выбрать сразу несколько файлов. Если программа не найдёт места для заполнения, она попросит показать их в Word.</p>
            <label className="primaryBtn fileBtn largeAction">
              Выбрать DOCX/DOCM
              <input type="file" accept=".docx,.docm" multiple onChange={props.onPickFile} data-testid="template-file-input" style={{ display: 'none' }} />
            </label>
            <details className="manualScannerDetails">
              <summary>Создать шаблон из вставленного текста</summary>
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
              <table className="confirm">
                <tbody>
                  <tr><th>Документ</th><td>{props.previewTitle}</td></tr>
                  <tr>
                    <th>Название кнопки</th>
                    <td><input value={props.buttonLabel} placeholder={props.previewTitle} onChange={(event) => props.onButtonLabelChange(event.target.value)} /></td>
                  </tr>
                </tbody>
              </table>
              <button className="softBtn" onClick={props.onAnalyze}>Анализировать</button>
              {props.templateText.trim() && !manualReady ? (
                <div className="readyMessage templateReadyMessage warning">
                  <i className="ti ti-alert-triangle" aria-hidden="true" />
                  <div>
                    <strong>Нужно указать места заполнения</strong>
                    <span>Выделите примерное значение ниже и замените его смысловым полем. Иначе текст примера мог бы попасть в новый документ.</span>
                  </div>
                </div>
              ) : null}
              <details className="manualScannerDetails">
                <summary>Дополнительная разметка</summary>
                <ScannerToolbar
                  selection={selection?.text ?? ''}
                  fieldId={scannerField}
                  onFieldIdChange={setScannerField}
                  onReplace={() => applyVisualMarkup('replace')}
                  onInsert={() => applyVisualMarkup('insert_after')}
                />
                <PopupFieldEditor compact fields={props.draftPopupFields} onChange={props.onDraftPopupFieldsChange} />
              </details>
            </details>
          </div>
        ) : (
          <>
            <div className="templateBatch" aria-label="Подготовленные шаблоны">
              <div className="templateBatchHead">2. Проверьте названия кнопок</div>
              {props.pendingTemplates.map((item) => (
                <div className={activePending?.document_id === item.document_id ? 'templateBatchRow selected' : 'templateBatchRow'} key={item.document_id}>
                  <button
                    className="templateFileSelect"
                    type="button"
                    title="Открыть дополнительную настройку"
                    onClick={() => { setActivePendingId(item.document_id); setSelection(null); }}
                  >
                    {item.file_name}
                  </button>
                  <input
                    aria-label={`Название документа для ${item.file_name}`}
                    value={item.button_label}
                    onChange={(event) => props.onPendingTemplateLabelChange(item.document_id, event.target.value)}
                  />
                </div>
              ))}
            </div>

            <div className={`readyMessage templateReadyMessage ${batchReady ? '' : 'warning'}`}>
              <i className={batchReady ? 'ti ti-circle-check' : 'ti ti-alert-triangle'} aria-hidden="true" />
              <div>
                <strong>{batchReady ? '3. Всё готово' : '3. Нужна разметка'}</strong>
                <span>
                  {batchReady
                    ? `Нажмите «${confirmLabel}». Шаблон задаёт форму, а значения будут взяты только из исходного документа и подтверждённых ответов.`
                    : `В ${unmarkedTemplateCount} шаблон(ах) не найдено подтверждённых мест заполнения. Откройте дополнительную настройку и покажите хотя бы одно место в Word. Текст примера не будет скопирован как данные нового документа.`}
                </span>
              </div>
            </div>

            {activePending ? (
              <details className="manualScannerDetails templateAdvancedSetup">
                <summary>Дополнительная настройка выбранного шаблона</summary>
                <div className="pendingCursorScanner">
                  <div className="guidedTemplateLaunch">
                    <div>
                      <strong>Показать места для автоматического заполнения</strong>
                      <small>Для неразмеченного шаблона покажите хотя бы одно место заполнения.</small>
                    </div>
                    <button className="softBtn" type="button" onClick={() => props.onStartGuidedPendingScanner(activePending.document_id)}>
                      <i className="ti ti-hand-click" aria-hidden="true" /> Открыть Word и показать место
                    </button>
                  </div>
                  <details className="manualScannerDetails">
                    <summary>Ручная разметка для опытных пользователей</summary>
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
                    <ScannerToolbar
                      selection={selection?.text ?? ''}
                      fieldId={scannerField}
                      onFieldIdChange={setScannerField}
                      disabled={marking}
                      onReplace={() => void applyPendingVisualMarkup('replace')}
                      onInsert={() => void applyPendingVisualMarkup('insert_after')}
                    />
                  </details>
                  <PopupFieldEditor
                    compact
                    fields={activePending.popup_fields}
                    onChange={(fields) => props.onPendingPopupFieldsChange(activePending.document_id, fields)}
                  />
                </div>
              </details>
            ) : null}

            <label className="softBtn fileBtn">
              Добавить ещё шаблоны
              <input type="file" accept=".docx,.docm" multiple onChange={props.onPickFile} style={{ display: 'none' }} />
            </label>
          </>
        )}

        <datalist id="template-scanner-field-suggestions">
          <option value="document.number" /><option value="document.date" /><option value="subject.name" /><option value="org.name" />
          <option value="amount.total" /><option value="period.start_date" /><option value="period.end_date" /><option value="related.number" />
        </datalist>

        <div className="modalActions">
          <span className="spacer" />
          <button className="softBtn" onClick={props.onCancel}>Отмена</button>
          <button className="primaryBtn" onClick={props.onConfirm} disabled={hasBatch ? !batchReady : !manualReady}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

function hasConfirmedPlaceholder(text: string): boolean {
  return /\{\{\s*[a-zA-Z0-9_.-]+\s*\}\}/.test(text);
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
