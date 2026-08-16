import type { CreatedDocumentOutput, DocumentTemplateSpec, FolderNamePartDto, GeneratedPrintItem, GuidedScannerMarkupAction, PopupFieldConfig, PromptSpec, WordScannerCapture, WordScannerSession, DomainKind } from './types';
import { newPopupField } from '../components/PopupFieldEditor';
import type { ScannerFieldSuggestion } from './scannerSuggestions';

export const DEFAULT_YEAR = new Date().getFullYear();
export const STATE_DB = 'dokkomplekt-user-state.sqlite';
export const OUTPUT_PREFS_KEY = 'dokkomplekt.output-folder-parts.v1';
export const OUTPUT_ROOT_KEY = 'dokkomplekt.output-root.v1';
export const OUTPUT_NAMING_CONFIRMED_KEY = 'dokkomplekt.output-folder-naming-confirmed.v1';
export const AUTO_PRINT_KEY = 'dokkomplekt.auto-print.v1';
export const PRINT_COPIES_KEY = 'dokkomplekt.print-copies.v1';

export function shouldSelectDocumentByDefault(document: DocumentTemplateSpec): boolean {
  if (document.category !== 'Medical') return true;
  const role = document.role_id.trim().toLowerCase();
  return !(
    role === 'discharge'
    || role.endsWith('.discharge')
    || role === 'diary'
    || role === 'diaries'
    || role.endsWith('.diary')
    || role.endsWith('.diaries')
  );
}


export function defaultSelectedDocumentIds(documents: DocumentTemplateSpec[]): string[] {
  return documents.filter(shouldSelectDocumentByDefault).map((document) => document.id);
}

export type PendingTemplate = {
  document_id: string;
  template_path: string;
  extracted_text: string;
  file_name: string;
  button_label: string;
  popup_fields: PopupFieldConfig[];
  domain_override: DomainKind | null;
};

export function withPendingTemplateDomain(
  items: PendingTemplate[],
  documentId: string,
  value: DomainKind | null,
): PendingTemplate[] {
  return items.map((item) => item.document_id === documentId
    ? { ...item, domain_override: value }
    : item);
}

export type PendingGeneration = {
  kind: 'single' | 'batch';
  documentIds: string[];
};

export type GuidedScannerTarget =
  | { mode: 'source'; documentId: string | null; label: string | null; domain: DomainKind | null }
  | { mode: 'template'; kind: 'pending' | 'existing'; documentId: string; label: string; domain: DomainKind | null };

export type GuidedScannerState = {
  session: WordScannerSession;
  target: GuidedScannerTarget;
  capture: WordScannerCapture | null;
  suggestions: ScannerFieldSuggestion[];
  selectedFieldId: string;
  rememberRule: boolean;
  addQuestion: boolean;
  markupAction: GuidedScannerMarkupAction;
};

export function inferGuidedMarkupAction(capture: WordScannerCapture): GuidedScannerMarkupAction {
  const selected = capture.selected_text.trim();
  const looksLikeLabel = /[:：№#]\s*$/.test(selected)
    || /^(?:номер|дата|фио|ф\.и\.о|инн|кпп|огрн|адрес|телефон|должность|диагноз|лечение|сумма|итого|vin)\b/i.test(selected);
  return looksLikeLabel ? 'insert_after' : 'replace';
}

export function ensureSuggestedPopupField(
  fields: PopupFieldConfig[],
  fieldId: string,
  title: string,
  inputKind: PopupFieldConfig['input_kind'],
): PopupFieldConfig[] {
  if (fields.some((field) => field.field_id === fieldId)) return fields;
  return [...fields, {
    ...newPopupField(fieldId),
    title,
    input_kind: inputKind,
    help_text: 'Если программа не найдёт это значение в исходном документе, она попросит специалиста его ввести.',
  }];
}

export function promptToPopupField(prompt: PromptSpec): PopupFieldConfig {
  return {
    field_id: prompt.field_id,
    title: prompt.title,
    required: prompt.required,
    input_kind: prompt.input_kind ?? 'text',
    ask_mode: prompt.ask_mode ?? 'if_missing',
    options: prompt.options ?? [],
    allow_custom_option: prompt.allow_custom_option ?? false,
    help_text: prompt.help_text ?? prompt.validation_hint ?? null,
    section: prompt.section ?? 'Данные документа',
    default_value: null,
    linked_to: prompt.linked_to ?? null,
    order: prompt.order ?? 500,
  };
}


export function loadOutputRoot(): string {
  try {
    const value = localStorage.getItem(OUTPUT_ROOT_KEY)?.trim();
    if (value) return value;
  } catch { /* use generic local default */ }
  // First run must ask for a real user-visible destination. A relative
  // application working-directory path is impossible for an end user to locate.
  return '';
}

export function saveOutputRoot(value: string): void {
  const normalized = value.trim();
  if (!normalized) return;
  try { localStorage.setItem(OUTPUT_ROOT_KEY, normalized); } catch { /* storage may be unavailable */ }
}

export function loadOutputNamingConfirmed(): boolean {
  try { return localStorage.getItem(OUTPUT_NAMING_CONFIRMED_KEY) === 'true'; } catch { return false; }
}

export function saveOutputFolderParts(parts: FolderNamePartDto[], confirmed = true): void {
  if (!parts.length) return;
  try {
    localStorage.setItem(OUTPUT_PREFS_KEY, JSON.stringify(parts));
    if (confirmed) localStorage.setItem(OUTPUT_NAMING_CONFIRMED_KEY, 'true');
  } catch { /* storage may be unavailable */ }
}

export function loadOutputFolderParts(): FolderNamePartDto[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(OUTPUT_PREFS_KEY) || 'null');
    if (Array.isArray(parsed) && parsed.every((value) => typeof value === 'string')) {
      return parsed as FolderNamePartDto[];
    }
  } catch { /* use privacy-safe default */ }
  return ['DocumentNumber', 'DocumentDate'];
}

export function loadAutoPrintPreference(): boolean {
  try { return localStorage.getItem(AUTO_PRINT_KEY) === 'true'; } catch { return false; }
}

export function loadPrintCopyPreferences(): Record<string, number> {
  try {
    const parsed = JSON.parse(localStorage.getItem(PRINT_COPIES_KEY) || '{}');
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return Object.fromEntries(Object.entries(parsed).map(([key, value]) => [key, normalizeCopyCount(Number(value))]));
    }
  } catch { /* use one copy by default */ }
  return {};
}

export function normalizeCopyCount(value: number): number {
  if (!Number.isFinite(value)) return 1;
  return Math.max(0, Math.min(99, Math.trunc(value)));
}

export function fileLabel(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop()?.replace(/\.[^.]+$/, '') || 'Документ';
}

export function cursorMarkedTemplatePath(inputPath: string, documentId: string): string {
  const extension = inputPath.match(/\.[^./\\]+$/)?.[0] ?? '.docx';
  const base = inputPath.slice(0, -extension.length);
  const safeId = documentId.replace(/[^a-zA-Z0-9_-]/g, '_');
  return `${base}.cursor-${safeId}-${Date.now()}${extension}`;
}

export function replaceAllLiteral(source: string, needle: string, replacement: string): string {
  return needle ? source.split(needle).join(replacement) : source;
}

export function createdPrintItems(
  created: CreatedDocumentOutput[] | undefined,
  paths: string[],
  documents: DocumentTemplateSpec[],
  requestedIds: string[] = [],
): GeneratedPrintItem[] {
  if (created?.length) return created.map((item) => ({ ...item }));
  return paths.map((path, index) => {
    const documentId = requestedIds[index] ?? `generated:${index}`;
    const document = documents.find((item) => item.id === documentId);
    return { document_id: documentId, label: document?.button_label ?? fileLabel(path), path };
  });
}

/** File.arrayBuffer с fallback на FileReader (нужен для jsdom в тестах). */
export function readFileBytes(file: File): Promise<ArrayBuffer> {
  if (typeof file.arrayBuffer === 'function') return file.arrayBuffer();
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as ArrayBuffer);
    reader.onerror = () => reject(reader.error ?? new Error('file read failed'));
    reader.readAsArrayBuffer(file);
  });
}

export function arrayBufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export function newDocumentId(): string {
  const random = globalThis.crypto?.randomUUID?.() ?? `${Date.now()}_${Math.random().toString(36).slice(2)}`;
  return `template_${random.replace(/[^a-zA-Z0-9_-]/g, '')}`;
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  try { return JSON.stringify(error); } catch { return 'Неизвестная ошибка'; }
}

export function detectTitle(text: string): string | null {
  for (const raw of text.split(/\r?\n/).slice(0, 20)) {
    const line = raw.trim();
    if (!line) continue;
    return line.replace(/\s*\{\{[^}]+\}\}.*/, '').replace(/^\d{1,2}[./-]\d{1,2}[./-]\d{2,4}\s+/, '').trim() || line;
  }
  return null;
}
