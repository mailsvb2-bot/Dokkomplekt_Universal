import type { DocumentTemplateSpec, PopupFieldConfig } from '../lib/types';
import { PopupFieldEditor } from './PopupFieldEditor';

interface PopupDesignerModalProps {
  busy: boolean;
  document: DocumentTemplateSpec;
  fields: PopupFieldConfig[];
  onChange(fields: PopupFieldConfig[]): void;
  onCancel(): void;
  onSave(): void;
}

export function PopupDesignerModal({ busy, document, fields, onChange, onCancel, onSave }: PopupDesignerModalProps) {
  return (
    <div className="backdrop" role="dialog" aria-modal="true" aria-label="Конструктор уточняющих вопросов">
      <div className="modal popupDesignerModal">
        <h2>Вопросы для «{document.button_label}»</h2>
        <fieldset disabled={busy} className="modalInteractionGuard">
        <p className="hint">
          Настройте данные, которые нужно уточнять перед созданием этого документа. Общие поля спрашиваются один раз на весь комплект и затем используются во всех связанных документах.
        </p>
        <PopupFieldEditor fields={fields} onChange={onChange} />
        <div className="modalActions">
          <span className="spacer" />
          <button className="softBtn" onClick={onCancel}>Отмена</button>
          <button className="primaryBtn" onClick={onSave} disabled={busy}>Сохранить вопросы</button>
        </div>
        </fieldset>
      </div>
    </div>
  );
}
