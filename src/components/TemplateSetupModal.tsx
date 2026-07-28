import type { ChangeEvent, DragEvent } from 'react';
import type { PopupFieldConfig } from '../lib/types';
import { PopupFieldEditor } from './PopupFieldEditor';

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
  const ready = props.pendingTemplates.length > 0;
  const confirmLabel = `Создать кнопки (${props.pendingTemplates.length})`;

  return (
    <div className="backdrop" role="dialog" aria-modal="true" aria-label="Добавление шаблонов">
      <div className="modal fileDropZone templateSetupSimple" onDragOver={(event: DragEvent<HTMLDivElement>) => event.preventDefault()} onDrop={(event) => {
        event.preventDefault();
        const files = Array.from(event.dataTransfer.files ?? []);
        if (files.length) props.onDropFiles(files);
      }}>
        <h2>Создать кнопки документов</h2>
        <p className="hint">Выберите шаблоны Word. Один файл станет одной кнопкой.</p>

        {!ready ? (
          <div className="emptyPackage templateFirstStep simpleTemplateStep">
            <div><i className="ti ti-file-upload" /></div>
            <h3>1. Выберите шаблоны</h3>
            <p>Можно выбрать сразу весь комплект. Предварительная разметка не нужна.</p>
            <label className="primaryBtn fileBtn largeAction">Выбрать DOCX/DOCM<input type="file" accept=".docx,.docm" multiple onChange={props.onPickFile} data-testid="template-file-input" style={{ display: 'none' }} /></label>
            <small>Также можно перетащить файлы прямо в это окно.</small>
          </div>
        ) : (
          <div className="simpleTemplateStep">
            <h3>2. Проверьте названия</h3>
            <div className="templateBatch" aria-label="Подготовленные шаблоны">
              {props.pendingTemplates.map((item) => (
                <label className="templateBatchRow" key={item.document_id}>
                  <span>{item.file_name}</span>
                  <input aria-label={`Название документа для ${item.file_name}`} value={item.button_label} onChange={(event) => props.onPendingTemplateLabelChange(item.document_id, event.target.value)} />
                </label>
              ))}
            </div>
            <div className="readyMessage templateReadyMessage"><i className="ti ti-circle-check" aria-hidden="true" /><div><strong>Всё готово</strong><span>Нажмите кнопку ниже. Обычные шаблоны без специальных полей тоже будут добавлены.</span></div></div>
            <button className="primaryBtn full" onClick={props.onConfirm}>{confirmLabel}</button>
            <label className="softBtn fileBtn">Добавить ещё шаблоны<input type="file" accept=".docx,.docm" multiple onChange={props.onPickFile} style={{ display: 'none' }} /></label>
          </div>
        )}

        <details className="manualScannerDetails simpleSetupAdvanced">
          <summary>Дополнительная настройка</summary>
          <p className="hint">Нужна только для нестандартных шаблонов. Она не блокирует создание кнопок.</p>
          {ready ? props.pendingTemplates.map((item) => (
            <details className="manualScannerDetails" key={item.document_id}>
              <summary>{item.button_label || item.file_name}</summary>
              <button className="softBtn" type="button" onClick={() => props.onStartGuidedPendingScanner(item.document_id)}>Открыть Word и показать места заполнения</button>
              <PopupFieldEditor compact fields={item.popup_fields} onChange={(fields) => props.onPendingPopupFieldsChange(item.document_id, fields)} />
            </details>
          )) : (
            <details className="manualScannerDetails">
              <summary>Создать кнопку из текста</summary>
              <textarea value={props.templateText} onChange={(event) => props.onTemplateTextChange(event.target.value)} placeholder="Вставьте текст документа" />
              <input value={props.buttonLabel} placeholder={props.previewTitle} onChange={(event) => props.onButtonLabelChange(event.target.value)} />
              <button className="softBtn" onClick={props.onAnalyze}>Проверить текст</button>
              <PopupFieldEditor compact fields={props.draftPopupFields} onChange={props.onDraftPopupFieldsChange} />
              <button className="primaryBtn" onClick={props.onConfirm} disabled={!props.templateText.trim()}>Создать кнопку</button>
            </details>
          )}
        </details>

        <div className="modalActions"><span className="spacer" /><button className="softBtn" onClick={props.onCancel}>Закрыть</button></div>
      </div>
    </div>
  );
}
