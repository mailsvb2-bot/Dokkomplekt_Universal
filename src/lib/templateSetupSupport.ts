import type { DomainKind, PopupFieldConfig, TemplateConfirmationRowDto } from './types';
import type { PendingTemplate } from './appSupport';

export function buildTemplateConfirmationRows(
  rows: TemplateConfirmationRowDto[],
  pendingTemplates: PendingTemplate[],
  buttonLabel: string,
  draftPopupFields: PopupFieldConfig[],
  draftDomainOverride: DomainKind | null,
  draftPopupFieldsEdited = false,
): Array<TemplateConfirmationRowDto & {
  domain_override: DomainKind | null;
  domain_override_is_explicit: boolean;
  popup_fields_edited: boolean;
}> {
  const pendingById = new Map(pendingTemplates.map((item) => [item.document_id, item]));
  return rows.map((row) => {
    const pending = pendingById.get(row.document_id);
    const popupFieldsEdited = pending
      ? Boolean(pending.popup_fields_edited)
      : rows.length === 1 && draftPopupFieldsEdited;
    const userDomainOverride = pending
      ? pending.domain_override
      : rows.length === 1
        ? draftDomainOverride
        : null;
    return {
      ...row,
      editable_button_label: pending?.button_label.trim()
        || (rows.length === 1 ? buttonLabel.trim() : '')
        || row.editable_button_label,
      popup_fields: popupFieldsEdited
        ? (pending?.popup_fields ?? draftPopupFields)
        : (row.popup_fields ?? []),
      popup_fields_edited: popupFieldsEdited,
      domain_override: userDomainOverride
        ?? row.domain_override
        ?? null,
      domain_override_is_explicit: userDomainOverride !== null
        || Boolean(row.domain_override_is_explicit),
    };
  });
}

export function templateSetupCompletionMessage(requestedCount: number, createdCount: number): string {
  const requested = Math.max(0, requestedCount);
  const created = Math.max(0, Math.min(createdCount, requested));
  const skipped = requested - created;

  if (created === 0 && skipped > 0) {
    return `Новых кнопок не создано. Повторяющихся шаблонов пропущено: ${skipped}.`;
  }
  if (skipped > 0) {
    return `Создано кнопок: ${created}. Повторяющихся шаблонов пропущено: ${skipped}. Теперь добавьте исходный документ.`;
  }
  return `Кнопки созданы: ${created}. Теперь добавьте исходный документ.`;
}


type PickedTemplateLike = {
  file_name: string;
  template_path: string;
  import_error?: string | null;
};

export function partitionPickedTemplates<T extends PickedTemplateLike>(files: T[]) {
  const rejectedTemplates = files.filter(file => Boolean(file.import_error));
  const acceptedTemplates = files.filter(file => !file.import_error && file.template_path.trim());
  const rejectedDetails = rejectedTemplates.map(file => `${file.file_name}: ${file.import_error}`).join('; ');
  return { acceptedTemplates, rejectedTemplates, rejectedDetails };
}

export function templatePickerCompletionMessage(createdCount: number, rejected: PickedTemplateLike[]): string {
  const rejectedSummary = rejected.length
    ? ` Пропущено проблемных шаблонов: ${rejected.length} — ${rejected.map(file => `${file.file_name}: ${file.import_error}`).join('; ')}.`
    : '';
  return `Шаблоны выбраны: ${createdCount}.${rejectedSummary} Проверьте названия и нажмите «Создать кнопки».`;
}
