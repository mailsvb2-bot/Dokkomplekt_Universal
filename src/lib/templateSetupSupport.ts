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
