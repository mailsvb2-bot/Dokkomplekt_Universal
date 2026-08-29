import type { AppConfirmOptions } from '../components/AppDialogProvider';
import {
  analyzeTemplateFile,
  applyTemplateLearningMap,
  applyTemplateMarkup,
  importLearningExampleFile,
  learnTemplateFromExamples,
} from './api';
import {
  currentDefaultYear,
  arrayBufferToBase64,
  cursorMarkedTemplatePath,
  readFileBytes,
  replaceAllLiteral,
  type PendingTemplate,
} from './appSupport';

type StateSetter<T> = (value: T | ((previous: T) => T)) => void;
type RunAction = <T>(label: string, action: () => Promise<T>) => Promise<T | undefined>;

interface PendingTemplateIntelligenceContext {
  pendingTemplates: PendingTemplate[];
  setPendingTemplates: StateSetter<PendingTemplate[]>;
  importedTemplatePath: string | null;
  setImportedTemplatePath: StateSetter<string | null>;
  templateText: string;
  setTemplateText: StateSetter<string>;
  setStatus: StateSetter<string>;
  run: RunAction;
  confirm(options: AppConfirmOptions): Promise<boolean>;
}

export function createPendingTemplateIntelligenceHandlers(context: PendingTemplateIntelligenceContext) {
  async function markupPendingTemplate(
    documentId: string,
    selectedText: string,
    fieldId: string,
    action: 'replace' | 'insert_after',
  ) {
    const current = context.pendingTemplates.find((item) => item.document_id === documentId);
    const value = selectedText.trim();
    const normalizedField = fieldId.trim();
    if (!current || !value || !normalizedField) {
      context.setStatus('Выделите значение в шаблоне и укажите смысловое поле.');
      return;
    }
    const outputPath = cursorMarkedTemplatePath(current.template_path, documentId);
    const report = await context.run('apply_template_markup_command', () => applyTemplateMarkup(
      current.template_path,
      outputPath,
      [{ field_id: normalizedField, value, action }],
    ));
    if (!report) return;
    if (!report.replaced_occurrences) {
      context.setStatus('Выделенный фрагмент не найден в видимом тексте DOCX/DOCM. Исходный шаблон не изменён.');
      return;
    }
    const placeholder = `{{${normalizedField}}}`;
    const visibleReplacement = action === 'replace' ? placeholder : `${value}${placeholder}`;
    context.setPendingTemplates((previous) => previous.map((item) => item.document_id === documentId
      ? {
          ...item,
          template_path: report.output_path,
          extracted_text: replaceAllLiteral(item.extracted_text, value, visibleReplacement),
        }
      : item));
    if (context.importedTemplatePath === current.template_path) context.setImportedTemplatePath(report.output_path);
    if (context.templateText === current.extracted_text) {
      context.setTemplateText(replaceAllLiteral(context.templateText, value, visibleReplacement));
    }
    context.setStatus(`Шаблон размечен. Обновлено мест: ${report.replaced_occurrences}. Исходный файл сохранён.`);
  }

  async function learnPendingTemplateFromExamples(documentId: string, files: File[]) {
    const current = context.pendingTemplates.find((item) => item.document_id === documentId);
    if (!current) return;
    if (files.length < 3 || files.length > 10) {
      context.setStatus('Для обучения выберите от 3 до 10 заполненных примеров одного и того же шаблона.');
      return;
    }

    const completedExamplePaths: string[] = [];
    for (const file of files) {
      const buffer = await readFileBytes(file);
      const imported = await context.run('import_learning_example_file', () =>
        importLearningExampleFile(file.name, arrayBufferToBase64(buffer)));
      if (!imported) return;
      completedExamplePaths.push(imported.source_path);
    }
    const learned = await context.run('learn_template_from_examples_command', () => learnTemplateFromExamples({
      blankTemplatePath: current.template_path,
      completedExamplePaths,
      defaultYear: currentDefaultYear(),
    }));
    if (!learned) return;
    const confidentFields = learned.fields.filter((field) => field.confidence >= 0.9);
    if (!confidentFields.length) {
      context.setStatus('Примеры изучены, но однозначных полей не найдено. Шаблон не изменён — используйте ручную разметку или покажите место в Word.');
      return;
    }
    const previewFields = confidentFields
      .slice(0, 8)
      .map((field) => `${field.field_id} (${Math.round(field.confidence * 100)}%)`)
      .join(', ');
    const accepted = await context.confirm({
      title: 'Применить найденную карту шаблона?',
      message: `Найдено надёжных полей: ${confidentFields.length}. ${previewFields}${confidentFields.length > 8 ? '…' : ''}. Будет создана новая размеченная копия; исходный Word останется неизменным.`,
      confirmLabel: 'Применить карту',
    });
    if (!accepted) {
      context.setStatus('Обучение отменено на этапе подтверждения. Исходный шаблон не изменён.');
      return;
    }

    const outputPath = cursorMarkedTemplatePath(current.template_path, `${documentId}-learned`);
    const applied = await context.run('apply_template_learning_map', () => applyTemplateLearningMap(
      current.template_path,
      outputPath,
      confidentFields.map((field) => ({
        field_id: field.field_id,
        line_index: field.line_index,
        blank_line: field.blank_line,
        common_prefix: field.common_prefix,
        common_suffix: field.common_suffix,
      })),
    ));
    if (!applied) return;
    if (!applied.applied_field_ids.length) {
      context.setStatus('Карта не смогла однозначно примениться к шаблону. Исходный файл не изменён.');
      return;
    }
    const analyzed = await context.run('analyze_template_file', () =>
      analyzeTemplateFile(applied.output_path, current.document_id, current.button_label));
    if (!analyzed) return;
    context.setPendingTemplates((previous) => previous.map((item) => item.document_id === documentId
      ? {
          ...item,
          template_path: applied.output_path,
          extracted_text: analyzed.extracted_text,
          popup_fields: analyzed.document.popup_fields ?? item.popup_fields,
        }
      : item));
    if (context.importedTemplatePath === current.template_path) context.setImportedTemplatePath(applied.output_path);
    if (context.templateText === current.extracted_text) context.setTemplateText(analyzed.extracted_text);
    context.setStatus(`Шаблон обучен: подтверждено и размечено полей — ${applied.applied_field_ids.length}. Исходный Word сохранён без изменений.`);
  }

  return { markupPendingTemplate, learnPendingTemplateFromExamples };
}
