import type { DomainKind, PopupFieldConfig, TemplateConfirmationRowDto } from './types';
import { arrayBufferToBase64, errorMessage, newDocumentId, type PendingTemplate } from './appSupport';


export function templateButtonLabelFromFileName(fileName: string): string {
  let stem = fileName.replace(/\.doc[xm]$/i, '').trim();
  // Volatile date/time prefixes belong to a concrete case, not the reusable button.
  stem = stem.replace(/^\d{1,2}[./-]\d{1,2}[./-]\d{2,4}\s+/, '');
  stem = stem.replace(/^\d{1,2}[:.]\d{2}\s+/, '');
  // Patient examples are often saved as `Фамилия И.О. тип документа.docx`.
  // The patient identity is source data, never a button name.
  stem = stem.replace(/^[А-ЯЁ][А-ЯЁа-яё-]+\s+[А-ЯЁ]\.\s*[А-ЯЁ]\.\s+/u, '');
  stem = stem.replace(/\s+(?:№|N|#)\s*\d+\s*$/i, '');
  stem = stem.trim().replace(/\s+/g, ' ');
  return stem || 'Документ';
}

function templateLabelCollisionKey(value: string): string {
  return value.trim().replace(/\s+/g, ' ').replace(/ё/gi, 'е').toLocaleLowerCase('ru-RU');
}

export function uniqueTemplateButtonLabel(base: string, used: Set<string>): string {
  const clean = base.trim().replace(/\s+/g, ' ') || 'Документ';
  const key = templateLabelCollisionKey(clean);
  if (!used.has(key)) {
    used.add(key);
    return clean;
  }
  for (let index = 2; ; index += 1) {
    const candidate = `${clean} ${index}`;
    const candidateKey = templateLabelCollisionKey(candidate);
    if (!used.has(candidateKey)) {
      used.add(candidateKey);
      return candidate;
    }
  }
}

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


export type BrowserTemplateImportFailure = {
  file_name: string;
  template_path: string;
  import_error: string;
};

export async function importBrowserTemplateFiles(
  files: File[],
  deps: {
    readFileBytes(file: File): Promise<ArrayBuffer>;
    importTemplateFile(documentId: string, source: { fileName: string; bytesBase64: string }): Promise<{ template_path: string; extracted_text: string }>;
    analyzeTemplateFile(templatePath: string, documentId: string, buttonLabel: string): Promise<{ document: { popup_fields?: PopupFieldConfig[] } }>;
  },
): Promise<{ importedRows: PendingTemplate[]; rejectedTemplates: BrowserTemplateImportFailure[] }> {
  const importedRows: PendingTemplate[] = [];
  const rejectedTemplates: BrowserTemplateImportFailure[] = [];
  for (const file of files) {
    if (!/\.doc[xm]$/i.test(file.name)) {
      rejectedTemplates.push({ file_name: file.name, template_path: '', import_error: 'Неподдерживаемый формат: нужен DOCX или DOCM' });
      continue;
    }
    try {
      const id = newDocumentId();
      const buffer = await deps.readFileBytes(file);
      const imported = await deps.importTemplateFile(id, { fileName: file.name, bytesBase64: arrayBufferToBase64(buffer) });
      const detectedLabel = templateButtonLabelFromFileName(file.name);
      const analyzed = await deps.analyzeTemplateFile(imported.template_path, id, detectedLabel);
      importedRows.push({
        document_id: id,
        template_path: imported.template_path,
        extracted_text: imported.extracted_text,
        file_name: file.name,
        button_label: detectedLabel,
        popup_fields: analyzed.document.popup_fields ?? [],
        domain_override: null,
      });
    } catch (error) {
      rejectedTemplates.push({ file_name: file.name, template_path: '', import_error: errorMessage(error) });
    }
  }
  return { importedRows, rejectedTemplates };
}
