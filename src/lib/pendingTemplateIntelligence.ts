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

export interface TemplateLearningPair {
  source: File;
  completed: File;
}

export function sourceEvidencedLearningFields<T extends { source_matches: string[] }>(fields: T[]): T[] {
  return fields.filter((field) => field.source_matches.some((value) => value.trim().length > 0));
}

export function publicationEligibleLearningFields<T extends { source_matches: string[] }>(report: { fields: T[]; validation: { passed: boolean } }): T[] {
  if (!report.validation.passed) return [];
  return sourceEvidencedLearningFields(report.fields);
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

  async function learnPendingTemplateFromExamples(documentId: string, pairs: TemplateLearningPair[]) {
    const current = context.pendingTemplates.find((item) => item.document_id === documentId);
    if (!current) return;
    if (pairs.length < 4 || pairs.length > 10) {
      context.setStatus('Для доказательного обучения подготовьте от 4 до 10 пар: минимум 3 обучающие и 1 контрольная Source → Correct Output.');
      return;
    }

    const sourceExamplePaths: string[] = [];
    const completedExamplePaths: string[] = [];
    for (const pair of pairs) {
      const sourceBuffer = await readFileBytes(pair.source);
      const importedSource = await context.run('import_learning_example_file', () =>
        importLearningExampleFile(pair.source.name, arrayBufferToBase64(sourceBuffer)));
      if (!importedSource) return;
      sourceExamplePaths.push(importedSource.source_path);

      const completedBuffer = await readFileBytes(pair.completed);
      const importedCompleted = await context.run('import_learning_example_file', () =>
        importLearningExampleFile(pair.completed.name, arrayBufferToBase64(completedBuffer)));
      if (!importedCompleted) return;
      completedExamplePaths.push(importedCompleted.source_path);
    }

    const learned = await context.run('learn_template_from_examples_command', () => learnTemplateFromExamples({
      blankTemplatePath: current.template_path,
      completedExamplePaths,
      sourceExamplePaths,
      defaultYear: currentDefaultYear(),
    }));
    if (!learned) return;

    if (!learned.validation.passed) {
      context.setStatus(`Контрольная пара не прошла независимую проверку. Шаблон не изменён. ${learned.validation.reasons.join(' ')}`);
      return;
    }

    const evidencedFields = publicationEligibleLearningFields(learned);
    if (!evidencedFields.length) {
      context.setStatus('Пары изучены, но ни одно поле не подтверждено исходниками. Шаблон не изменён — добавьте более показательные пары или используйте ручную разметку.');
      return;
    }
    const previewFields = evidencedFields
      .slice(0, 8)
      .map((field) => `${field.field_id} (${field.source_matches.length} совпад.; оценка ${Math.round(field.confidence * 100)}%)`)
      .join(', ');
    const accepted = await context.confirm({
      title: 'Применить карту, подтверждённую примерами?',
      message: `Независимая контрольная пара пройдена: ${learned.validation.matched_fields}/${learned.validation.evaluated_fields} проверяемых полей, новых значений перенесено ${learned.validation.intervention_matches}/${learned.validation.intervention_fields}. Подтверждено полей для карты: ${evidencedFields.length}. ${previewFields}${evidencedFields.length > 8 ? '…' : ''}. Процент — только оценка приоритета, не доказательство. Будет создана новая размеченная копия; исходный Word останется неизменным.`,
      confirmLabel: 'Применить подтверждённую карту',
    });
    if (!accepted) {
      context.setStatus('Обучение отменено на этапе явного подтверждения. Исходный шаблон не изменён.');
      return;
    }

    const outputPath = cursorMarkedTemplatePath(current.template_path, `${documentId}-learned`);
    const applied = await context.run('apply_template_learning_map', () => applyTemplateLearningMap(
      current.template_path,
      outputPath,
      evidencedFields.map((field) => ({
        field_id: field.field_id,
        line_index: field.line_index,
        blank_line: field.blank_line,
        common_prefix: field.common_prefix,
        common_suffix: field.common_suffix,
      })),
    ));
    if (!applied) return;
    if (!applied.applied_field_ids.length) {
      context.setStatus('Подтверждённая карта не смогла однозначно примениться к шаблону. Исходный файл не изменён.');
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
    context.setStatus(`Шаблон обучен на ${pairs.length - 1} парах и проверен на 1 независимой контрольной паре: явно подтверждено и размечено полей — ${applied.applied_field_ids.length}. Исходный Word сохранён без изменений.`);
  }

  return { markupPendingTemplate, learnPendingTemplateFromExamples };
}
