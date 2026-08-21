import type { DomainKind, PopupFieldConfig, TemplateConfirmationRowDto } from './types';
import type { PendingTemplate } from './appSupport';

export function buildTemplateConfirmationRows(
  rows: TemplateConfirmationRowDto[],
  pendingTemplates: PendingTemplate[],
  buttonLabel: string,
  draftPopupFields: PopupFieldConfig[],
  draftDomainOverride: DomainKind | null,
): Array<TemplateConfirmationRowDto & { domain_override: DomainKind | null }> {
  const pendingById = new Map(pendingTemplates.map((item) => [item.document_id, item]));
  return rows.map((row) => {
    const pending = pendingById.get(row.document_id);
    return {
      ...row,
      editable_button_label: pending?.button_label.trim()
        || (rows.length === 1 ? buttonLabel.trim() : '')
        || row.editable_button_label,
      popup_fields: pending?.popup_fields
        ?? (rows.length === 1 ? draftPopupFields : row.popup_fields ?? []),
      domain_override: pending?.domain_override
        ?? (rows.length === 1 ? draftDomainOverride : null)
        ?? row.domain_override
        ?? null,
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
