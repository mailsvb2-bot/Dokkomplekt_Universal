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
        <h2>Добавить шаблоны документов</h2>
        <p className="hint">
          Выберите готовые формы DOCX/DOCM. Программа распознает их структуру, предложит понятные названия и запомнит, какие данные нужно подставлять.
        </p>

        {hasBatch ? (
          <>
            <div className="templateBatch" aria-label="Подготовленные шаблоны">
              <div className="templateBatchHead">Проверьте названия документов в наборе</div>
              {props.pendingTemplates.map((item) => (
                <div className={activePending?.document_id === item.document_id ? 'templateBatchRow selected' : 'templateBatchRow'} key={item.document_id}>
                  <button className="templateFileSelect" type="button" title="Открыть текст и вопросы" onClick={() => { setActivePendingId(item.document_id); setSelection(null); }}>
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
            {activePending ? (
              <>
                <div className="pendingCursorScanner">
                  <div className="templateBatchHead">Сканер курсором мыши — покажите программе место в шаблоне</div>
                  <div className="guidedTemplateLaunch">
                    <div>
                      <strong>Не нужно искать техническое имя поля</strong>
                      <small>Программа сама откроет безопасную копию Word, предложит варианты и сама закроет документ после разметки.</small>
                    </div>
                    <button className="primaryBtn" type="button" onClick={() => props.onStartGuidedPendingScanner(activePending.document_id)}>
                      <i className="ti ti-hand-click" aria-hidden="true" /> Открыть Word и показать место мышкой
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
                  <small className="hint">Размечается копия DOCX/DOCM. Исходный файл, форматирование, таблицы и макросы не перезаписываются.</small>
                </div>
                <PopupFieldEditor
                  compact
                  fields={activePending.popup_fields}
                  onChange={(fields) => props.onPendingPopupFieldsChange(activePending.document_id, fields)}
                />
              </>
            ) : null}
          </>
        ) : (
          <>
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
            />
            <ScannerToolbar
              selection={selection?.text ?? ''}
              fieldId={scannerField}
              onFieldIdChange={setScannerField}
              onReplace={() => applyVisualMarkup('replace')}
              onInsert={() => applyVisualMarkup('insert_after')}
            />
            <table className="confirm">
              <tbody>
                <tr><th>Выбранный документ</th><td>{props.previewTitle}</td></tr>
                <tr>
                  <th>Название в наборе</th>
                  <td><input value={props.buttonLabel} placeholder={props.previewTitle} onChange={(event) => props.onButtonLabelChange(event.target.value)} /></td>
                </tr>
              </tbody>
            </table>
            <PopupFieldEditor compact fields={props.draftPopupFields} onChange={props.onDraftPopupFieldsChange} />
          </>
        )}

        <datalist id="template-scanner-field-suggestions">
          <option value="document.number" /><option value="document.date" /><option value="subject.name" /><option value="org.name" />
          <option value="amount.total" /><option value="period.start_date" /><option value="period.end_date" /><option value="related.number" />
          <option value="amount.total" /><option value="period.start_date" /><option value="period.end_date" />
        </datalist>

        <div className="modalActions">
          {!hasBatch && <button className="softBtn" onClick={props.onAnalyze}>Анализировать</button>}
          <label className="softBtn fileBtn">
            Выбрать DOCX/DOCM
            <input type="file" accept=".docx,.docm" multiple onChange={props.onPickFile} data-testid="template-file-input" style={{ display: 'none' }} />
          </label>
          <span className="spacer" />
          <button className="softBtn" onClick={props.onCancel}>Отмена</button>
          <button className="primaryBtn" onClick={props.onConfirm}>
            {props.pendingTemplates.length > 1 ? `Добавить документы (${props.pendingTemplates.length})` : 'Добавить документ'}
          </button>
        </div>
      </div>
    </div>
  );
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
