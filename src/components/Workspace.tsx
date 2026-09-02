import { useState, type Dispatch, type DragEvent, type ReactNode, type SetStateAction } from 'react';
import type {
  CreatedDocumentsIntakeResult,
  DocumentTemplateSpec,
  GeneratedOutput,
  GeneratedPrintItem,
  IntakeCapability,
  PromptSpec,
  SemanticExtractResult,
  WorkflowPlan,
} from '../lib/types';
import { AdditionalMaterialsPanel } from './AdditionalMaterialsPanel';

interface ParsedSourceSummary {
  title: string;
  count: number;
  warnings: string[];
  sourceKind?: string;
  layoutRows?: number;
  tableRows?: number;
}

interface PreviewState {
  text: string;
  missing: number;
  label: string;
}

interface WorkspaceProps {
  busy: boolean;
  documents: DocumentTemplateSpec[];
  selectedDocumentIds: string[];
  watchFolder: string;
  intakeSource: string;
  intakeResult: CreatedDocumentsIntakeResult | null;
  lastOutput: GeneratedOutput | null;
  autoPrint: boolean;
  printCopies: Record<string, number>;
  sourceText: string;
  sourceFileName: string | null;
  sourceFilePath: string | null;
  webSourceUrl: string;
  intakeCapabilities: IntakeCapability[];
  scannerField: string;
  scannerText: string;
  parsed: ParsedSourceSummary | null;
  modelOutput: string;
  semantic: SemanticExtractResult | null;
  plan: WorkflowPlan | null;
  planLoading: boolean;
  selectedDocumentCount: number;
  activeDocumentLabel: string | null;
  showSickLeaveOption: boolean;
  sickLeaveEnabled: boolean;
  answers: Record<string, string>;
  skippedAnswers: Record<string, boolean>;
  preview: PreviewState | null;
  onPickWatchFolder(): void;
  onInstallWatcher(): void;
  onUninstallWatcher(): void;
  setIntakeSource(value: string): void;
  setAutoPrint(value: boolean): void;
  setSourceText(value: string): void;
  setSourceFileName(value: string | null): void;
  setWebSourceUrl(value: string): void;
  setScannerField(value: string): void;
  setScannerText(value: string): void;
  setModelOutput(value: string): void;
  setAnswers: Dispatch<SetStateAction<Record<string, string>>>;
  setSkippedAnswers: Dispatch<SetStateAction<Record<string, boolean>>>;
  onSickLeaveChange(value: boolean): void;
  onRunZeroTouch(): void;
  onOpenLastOutput(): void;
  onPrintLastOutput(): void;
  onExportLastOutputPdf(): void;
  onExportLastOutputPdfa(): void;
  onExportLastOutputKedo(): void;
  onPickSourceFile(): void;
  onDropSourceFile(file: File): void;
  onLoadWebSource(): void;
  onResetCase(): void;
  onParseSource(): void;
  onStartGuidedSourceScanner(): void;
  onReportSemanticError(fieldId: string, value: string): void;
  onApplyScannerSelection(): void;
  onApplyScannerAndQuestion(): void;
  onPrintCopyChange(documentId: string, copies: number): void;
  onUnderstand(): void;
  onPinField(fieldId: string): void;
  onPreview(): void;
  onCreateSelected(): void;
}

function sourceKindLabel(kind?: string): string {
  switch (kind) {
    case 'scanned_image': return 'изображение';
    case 'scanned_pdf_ocr': return 'сканированный PDF';
    case 'mixed_pdf_page_ocr': return 'PDF смешанного типа';
    case 'pdf_text': return 'PDF';
    case 'word': return 'Word';
    case 'legacy_word_converted': return 'документ Word';
    case 'presentation_converted': return 'презентация';
    case 'spreadsheet': return 'таблица';
    case 'archive': return 'архив';
    case 'email': return 'письмо';
    case 'https': return 'веб-источник';
    case 'manual_text': return 'вставленный текст';
    default: return kind || 'файл';
  }
}

function highlightedSource(text: string, evidence: string | undefined): ReactNode {
  const needle = evidence?.trim();
  if (!needle) return text;
  const index = text.toLocaleLowerCase().indexOf(needle.toLocaleLowerCase());
  if (index < 0) return text;
  return <>
    {text.slice(0, index)}
    <mark>{text.slice(index, index + needle.length)}</mark>
    {text.slice(index + needle.length)}
  </>;
}

export function Workspace(props: WorkspaceProps) {
  const [reviewFieldId, setReviewFieldId] = useState<string | null>(null);
  const reviewField = reviewFieldId
    ? props.semantic?.fields.find(field => field.field_id === reviewFieldId) ?? null
    : null;
  const reviewEvidence = reviewField?.evidence?.[0];
  const sourceReady = Boolean(props.sourceFileName || props.parsed);
  const prompts = props.plan?.prompts ?? [];
  const planReady = !props.planLoading && props.plan !== null;
  const reviewCount = props.semantic?.fields.filter(field => field.confidence < .95).length ?? 0;

  return (
    <main className="clientWorkspace">
      <section className="workflowHero" aria-labelledby="workflow-title">
        <div>
          <span className="workflowEyebrow">Новый комплект</span>
          <h1 id="workflow-title">Из исходника — готовые документы</h1>
          <p>Добавьте любой поддерживаемый файл. Программа извлечёт данные, попросит только недостающее и подготовит выбранный комплект.</p>
        </div>
        <div className="workflowHeroActions">
          <button className="softBtn newCaseBtn" onClick={props.onResetCase} disabled={props.busy}><i className="ti ti-file-plus" aria-hidden="true" /> Новый комплект</button>
          <ol className="workflowSteps" aria-label="Этапы работы">
            <li className={sourceReady ? 'done' : 'current'}><span>1</span><strong>Источник</strong></li>
            <li className={sourceReady && !props.lastOutput ? 'current' : props.lastOutput ? 'done' : ''}><span>2</span><strong>Проверка</strong></li>
            <li className={props.lastOutput ? 'current done' : ''}><span>3</span><strong>Результат</strong></li>
          </ol>
        </div>
      </section>

      {props.lastOutput && (
        <section className="resultCard" role="status" aria-label="Комплект готов">
          <div className="resultIcon"><i className="ti ti-check" aria-hidden="true" /></div>
          <div className="resultBody">
            <span className="resultEyebrow">Готово</span>
            <h2>Создано документов: {props.lastOutput.files.length}</h2>
            <p className="resultFolder" title={props.lastOutput.folder || props.lastOutput.files[0]}>
              <strong>Папка:</strong> {props.lastOutput.folder || props.lastOutput.files[0]}
            </p>
            <details className="resultFiles" open>
              <summary>Созданные файлы</summary>
              <ul>
                {props.lastOutput.files.map((path) => (
                  <li key={path} title={path}>{path.split(/[\\/]/).filter(Boolean).pop() || path}</li>
                ))}
              </ul>
            </details>
            {props.lastOutput.print_items?.length ? (
              <details className="resultCopies">
                <summary>Количество экземпляров для печати</summary>
                <div className="printCopyList">
                  {props.lastOutput.print_items.map((item: GeneratedPrintItem) => (
                    <label key={`${item.document_id}:${item.path}`} className="printCopyRow">
                      <span title={item.path}>{item.label}</span>
                      <input
                        type="number"
                        min={0}
                        max={99}
                        value={props.printCopies[item.document_id] ?? 1}
                        aria-label={`Количество экземпляров для ${item.label}`}
                        onChange={(event) => props.onPrintCopyChange(item.document_id, Number(event.target.value))}
                      />
                      <small>экз.</small>
                    </label>
                  ))}
                </div>
              </details>
            ) : null}
          </div>
          <div className="resultActions">
            <button className="primaryBtn" onClick={props.onOpenLastOutput} disabled={props.busy}><i className="ti ti-folder-open" aria-hidden="true" /> Открыть папку с документами</button>
            <button className="softBtn" onClick={props.onPrintLastOutput} disabled={props.busy}><i className="ti ti-printer" aria-hidden="true" /> Печать</button>
            <details className="moreActions">
              <summary aria-label="Дополнительные форматы"><i className="ti ti-dots" aria-hidden="true" /></summary>
              <div>
                <button onClick={props.onExportLastOutputPdf} disabled={props.busy}>Создать PDF</button>
                <button onClick={props.onExportLastOutputPdfa} disabled={props.busy}>Создать PDF/A</button>
                <button onClick={props.onExportLastOutputKedo} disabled={props.busy}>Создать пакет обмена</button>
              </div>
            </details>
          </div>
        </section>
      )}

      <section
        className={`sourceStage ${sourceReady ? 'ready' : ''}`}
        onDragOver={(event: DragEvent<HTMLElement>) => event.preventDefault()}
        onDrop={(event: DragEvent<HTMLElement>) => {
          event.preventDefault();
          if (props.busy) return;
          const file = event.dataTransfer.files?.[0];
          if (file) props.onDropSourceFile(file);
        }}
      >
        <div className="stageHeading">
          <span className="stageNumber">1</span>
          <div>
            <h2>{sourceReady ? 'Источник принят' : 'Добавьте исходный файл'}</h2>
            <p>{sourceReady ? 'Данные уже извлечены. При необходимости замените файл или проверьте распознанное.' : 'Перетащите файл сюда или выберите его на компьютере.'}</p>
          </div>
        </div>

        {!sourceReady ? (
          <div className="dropHero">
            <div className="dropIcon"><i className="ti ti-file-upload" aria-hidden="true" /></div>
            <strong>Перетащите документ в эту область</strong>
            <span>Word, PDF, изображение, таблица, письмо, архив и другие поддерживаемые форматы</span>
            <button className="primaryBtn fileBtn largeAction" type="button" aria-label="Выбрать исходный файл" onClick={props.onPickSourceFile} disabled={props.busy}>
              Выбрать файл
            </button>
          </div>
        ) : (
          <div className="sourceAccepted">
            <div className="sourceFileIcon"><i className="ti ti-file-check" aria-hidden="true" /></div>
            <div className="sourceFileInfo">
              <strong>{props.sourceFileName || props.parsed?.title || 'Вставленный текст'}</strong>
              <span>{sourceKindLabel(props.parsed?.sourceKind)}{props.parsed ? ` · найдено значений: ${props.parsed.count}` : ''}</span>
              {props.parsed?.warnings.length ? <em>Нужно проверить замечаний: {props.parsed.warnings.length}</em> : <em className="okText">Источник прочитан без критических замечаний</em>}
            </div>
            <div className="sourceActions">
              <button className="softBtn fileBtn" type="button" aria-label="Заменить исходный файл" onClick={props.onPickSourceFile} disabled={props.busy}>
                Заменить файл этого комплекта
              </button>
              <button className="textBtn" onClick={props.onResetCase} disabled={props.busy}>Новый пациент / дело</button>
              <small className="sourceCaseHint">Если это другой пациент или новое дело, начните новый комплект — так предыдущие данные и ручной выбор точно не будут считаться продолжением текущего.</small>
            </div>
          </div>
        )}

        <details className="alternativeSource">
          <summary>Другой способ добавить источник</summary>
          <div className="alternativeGrid">
            <div className="alternativeCard">
              <strong>Ссылка</strong>
              <p>Загрузить страницу, открытый файл или данные из API.</p>
              <div className="inlineInput">
                <input value={props.webSourceUrl} onChange={(event) => props.setWebSourceUrl(event.target.value)} placeholder="https://..." aria-label="Адрес источника" />
                <button className="softBtn" onClick={props.onLoadWebSource} disabled={props.busy || !props.webSourceUrl.trim()}>Загрузить</button>
              </div>
            </div>
            <div className="alternativeCard">
              <strong>Текст</strong>
              <p>Вставить содержимое вручную, если файла нет.</p>
              <textarea
      value={props.sourceText}
      onChange={(event) => props.setSourceText(event.target.value)}
      onSelect={(event) => {
        const target = event.currentTarget;
        const start = target.selectionStart ?? 0;
        const end = target.selectionEnd ?? start;
        if (end > start) props.setScannerText(target.value.slice(start, end));
      }}
      placeholder="Вставьте текст источника"
    />
              <button className="softBtn" onClick={props.onParseSource} disabled={props.busy || !props.sourceText.trim()}>Использовать текст</button>
            </div>
          </div>
        </details>
      </section>

      <AdditionalMaterialsPanel
        documents={props.documents}
        selectedDocumentIds={props.selectedDocumentIds}
        busy={props.busy}
        medicalDiagnosis={props.answers['medical.diagnosis']?.trim() || props.semantic?.fields.find((field) => field.field_id === 'medical.diagnosis')?.value?.trim() || ''}
      />

      {sourceReady && (
        <section className="reviewStage">
          <div className="stageHeading">
            <span className="stageNumber">2</span>
            <div>
              <h2>{props.planLoading
                ? 'Проверяем выбранный комплект…'
                : !props.selectedDocumentCount
                  ? 'Выберите документы комплекта'
                  : !planReady
                    ? 'План комплекта пока не готов'
                    : prompts.length
                      ? 'Уточните недостающие данные'
                      : 'Источник готов к созданию комплекта'}</h2>
              <p>{props.planLoading
                ? `Строим финальный план для выбранных документов: ${props.selectedDocumentCount}.`
                : !props.selectedDocumentCount
                  ? 'Сначала выберите хотя бы один документ справа.'
                  : !planReady
                    ? 'Готовность не объявляется, пока backend не подтвердит финальный generation-plan.'
                    : prompts.length
                      ? 'Показываем ровно те поля, которые потребуются при создании выбранного комплекта.'
                      : 'Финальный план проверен: обязательных уточнений для выбранного комплекта нет.'}</p>
            </div>
          </div>

          {props.planLoading ? (
            <div className="readyMessage" role="status">
              <i className="ti ti-loader-2" aria-hidden="true" />
              <div><strong>Сверяем шаблоны и найденные значения</strong><span>Сообщение о готовности появится только после расчёта финального generation-plan.</span></div>
            </div>
          ) : !props.selectedDocumentCount || !planReady ? (
            <div className="readyMessage notReady" role="status">
              <i className="ti ti-alert-circle" aria-hidden="true" />
              <div><strong>Создание пока не разрешено</strong><span>Выберите документы и дождитесь подтверждённого финального плана.</span></div>
            </div>
          ) : prompts.length ? (
            <div className="clientFields">
              {props.showSickLeaveOption && (
                <label className="checkLine workflowOption">
                  <input type="checkbox" checked={props.sickLeaveEnabled} onChange={(event) => props.onSickLeaveChange(event.target.checked)} />
                  <span>Оформляется больничный лист</span>
                </label>
              )}
              {prompts.map((prompt: PromptSpec) => (
                <WorkflowPromptField
                  key={prompt.field_id}
                  prompt={prompt}
                  value={props.answers[prompt.field_id] ?? prompt.current_value ?? ''}
                  skipped={Boolean(props.skippedAnswers[prompt.field_id])}
                  onChange={(value) => {
                    props.setSkippedAnswers((previous) => ({ ...previous, [prompt.field_id]: false }));
                    props.setAnswers((previous) => {
                      const previousSourceValue = previous[prompt.field_id] ?? prompt.current_value ?? '';
                      const next = { ...previous, [prompt.field_id]: value };
                      for (const linkedPrompt of prompts) {
                        if (linkedPrompt.linked_to !== prompt.field_id) continue;
                        const linkedCurrent = previous[linkedPrompt.field_id] ?? linkedPrompt.current_value ?? '';
                        if (!linkedCurrent || linkedCurrent === previousSourceValue) next[linkedPrompt.field_id] = value;
                      }
                      return next;
                    });
                  }}
                  onSkipChange={(skipped) => props.setSkippedAnswers((previous) => ({
                    ...previous,
                    [prompt.field_id]: skipped,
                  }))}
                  onPin={() => props.onPinField(prompt.field_id)}
                />
              ))}
              <div className="reviewActions">
                <button className="primaryBtn" onClick={props.onCreateSelected} disabled={props.busy || !planReady}>
                  {props.busy ? 'Создаём документы…' : `Проверить и создать (${props.selectedDocumentCount})`}
                </button>
                <button className="softBtn" onClick={props.onPreview} disabled={props.busy || !props.activeDocumentLabel}>
                  {props.activeDocumentLabel ? `Предпросмотр «${props.activeDocumentLabel}»` : 'Откройте документ для предпросмотра'}
                </button>
              </div>
            </div>
          ) : (
            <div className="readyMessage">
              <i className="ti ti-circle-check" aria-hidden="true" />
              <div><strong>Можно создавать документы</strong><span>{props.semantic ? `Распознано значений: ${props.semantic.fields.length}${reviewCount ? ` · рекомендуем проверить: ${reviewCount}` : ''}` : 'Данные источника будут проверены ещё раз перед сохранением.'}</span></div>
              <div className="readyActions">
                <button className="softBtn" onClick={props.onUnderstand} disabled={props.busy || !planReady || !props.selectedDocumentCount}>Проверить данные</button>
                <button className="primaryBtn" onClick={props.onCreateSelected} disabled={props.busy || !planReady || !props.selectedDocumentCount}>
                  {props.busy ? 'Создаём документы…' : `Создать документы (${props.selectedDocumentCount})`}
                </button>
              </div>
            </div>
          )}

          {props.preview && (
            <details className="clientPreview" open>
              <summary>Предпросмотр: {props.preview.label}{props.preview.missing ? ` · не заполнено: ${props.preview.missing}` : ''}</summary>
              <pre>{props.preview.text || '—'}</pre>
            </details>
          )}
        </section>
      )}

      <details className="automationCard">
        <summary>
          <span className="automationIcon"><i className="ti ti-bolt" aria-hidden="true" /></span>
          <span><strong>Автоматическая обработка папки</strong><small>Программа может сама замечать новые файлы и создавать комплект без ручного запуска.</small></span>
          <i className="ti ti-chevron-down" aria-hidden="true" />
        </summary>
        <div className="automationBody">
          <label><span>Рабочая папка</span><div className="inlineInput folderPicker"><input value={props.watchFolder} readOnly placeholder="Выберите абсолютную папку на компьютере" aria-label="Рабочая папка фонового агента" /><button className="softBtn" type="button" onClick={props.onPickWatchFolder} disabled={props.busy}><i className="ti ti-folder" aria-hidden="true" /> Выбрать</button></div><small>Фоновый агент принимает только явно выбранный абсолютный путь.</small></label>
          <div className="automationActions" aria-label="Управление автоматической обработкой">
            <button className="primaryBtn" type="button" onClick={props.onInstallWatcher} disabled={props.busy || !props.watchFolder.trim()}>
              <i className="ti ti-eye-cog" aria-hidden="true" /> Включить автоматическую обработку
            </button>
            <button className="softBtn" type="button" onClick={props.onUninstallWatcher} disabled={props.busy}>
              <i className="ti ti-eye-off" aria-hidden="true" /> Отключить автоматическую обработку
            </button>
          </div>
          <small className="automationHelp">После выбора рабочей папки нажмите «Включить автоматическую обработку». Эти же команды остаются доступны в настройках программы.</small>
          <label><span>Обработать файл по пути</span><div className="inlineInput"><input value={props.intakeSource} onChange={(event) => props.setIntakeSource(event.target.value)} placeholder="Путь к файлу" /><button className="primaryBtn" onClick={props.onRunZeroTouch} disabled={props.busy}>Создать комплект</button></div></label>
          <label className="checkLine"><input type="checkbox" checked={props.autoPrint} onChange={(event) => props.setAutoPrint(event.target.checked)} /><span>Печатать готовый комплект автоматически</span></label>
          <small className="automationHelp">Если файл временно нельзя прочитать, рядом появится заметка «НЕ ПРОЧИТАН.txt» с понятной причиной и временем следующей попытки.</small>
          {props.intakeResult && <div className={`automationResult ${props.intakeResult.status}`}><strong>{props.intakeResult.status === 'processed' ? 'Комплект создан' : props.intakeResult.status === 'attention' ? 'Нужно уточнение' : 'Информация'}</strong><span>{props.intakeResult.message}</span></div>}
        </div>
      </details>

      <details className="advancedCard">
        <summary><i className="ti ti-adjustments" aria-hidden="true" /> Расширенные инструменты</summary>
        <div className="advancedBody">
          <section>
            <h3>Точность распознавания</h3>
            <p>Проверьте найденные значения, источник каждого результата и при необходимости обучите правило на выделении.</p>
            <div className="advancedActions">
              <button className="softBtn" onClick={props.onUnderstand} disabled={props.busy}>Обновить распознанные данные</button>
              <button className="softBtn" onClick={props.onStartGuidedSourceScanner} disabled={props.busy || !props.sourceFilePath || !/\.doc[xm]$/i.test(props.sourceFilePath)}>Показать значение в Word</button>
            </div>
            {props.semantic && (
              <ul className="neutralDataList">
                {props.semantic.fields.map((field) => (
                  <li key={field.field_id}>
                    <div><strong>{field.value}</strong><small>{field.field_id} · уверенность {(field.confidence * 100).toFixed(0)}%</small></div>
                    <div><button className="textBtn" onClick={() => setReviewFieldId(field.field_id)}>Сверить</button><button className="textBtn" disabled={props.busy || !props.sourceFilePath} onClick={() => props.onReportSemanticError(field.field_id, field.value)}>Здесь ошибка</button></div>
                  </li>
                ))}
              </ul>
            )}
            {reviewField && <div className="evidenceReview">
              <div className="evidencePane"><strong>Фрагмент источника</strong><pre>{highlightedSource(props.sourceText, reviewEvidence)}</pre></div>
              <div className="evidencePane"><strong>Распознанное значение</strong><pre>{reviewField.value}</pre></div>
            </div>}
          </section>

          <section>
            <h3>Ручная разметка</h3>
            <div className="manualMarkup">
              <input list="known-field-ids" value={props.scannerField} onChange={(event) => props.setScannerField(event.target.value)} placeholder="Идентификатор поля" />
              <datalist id="known-field-ids"><option value="document.number" /><option value="document.date" /><option value="subject.name" /><option value="organization.name" /><option value="organization.inn" /></datalist>
              <input value={props.scannerText} onChange={(event) => props.setScannerText(event.target.value)} placeholder="Выделенный текст" />
              <button className="softBtn" onClick={props.onApplyScannerSelection} disabled={props.busy || !props.scannerText.trim() || !props.scannerField.trim()}>Назначить выделение полю</button>
              <button className="softBtn" onClick={props.onApplyScannerAndQuestion} disabled={props.busy || !props.scannerText.trim() || !props.scannerField.trim()}>Назначить и спрашивать при отсутствии</button>
            </div>
          </section>

          <section>
            <h3>Диагностика форматов</h3>
            {props.intakeCapabilities.length ? <ul className="capabilityList">{props.intakeCapabilities.map(item => <li key={item.format}><strong>{item.format}</strong><span>{item.ready ? 'готово' : 'требуется компонент'}</span><small>{item.detail}</small></li>)}</ul> : <p>Сведения появятся после проверки компонентов.</p>}
            <details className="modelDetails"><summary>Дополнительные данные распознавания</summary><textarea value={props.modelOutput} onChange={(event) => props.setModelOutput(event.target.value)} placeholder="Служебные данные в формате JSON" spellCheck={false} /></details>
          </section>
        </div>
      </details>
    </main>
  );
}

export function WorkflowPromptField(props: {
  prompt: PromptSpec;
  value: string;
  skipped: boolean;
  onChange(value: string): void;
  onSkipChange(value: boolean): void;
  onPin(): void;
  showPin?: boolean;
}) {
  const { prompt } = props;
  const kind = prompt.input_kind ?? 'text';
  const inputId = `workflow-${prompt.field_id.replace(/[^a-zA-Z0-9_-]/g, '-')}`;
  const options = prompt.options ?? [];
  const hint = prompt.validation_hint || prompt.help_text;
  let control: ReactNode;

  if (kind === 'long_text') {
    control = <textarea id={inputId} value={props.value} rows={4} disabled={props.skipped} onChange={(event) => props.onChange(event.target.value)} />;
  } else if (kind === 'yes_no') {
    control = (
      <select id={inputId} value={props.value} disabled={props.skipped} onChange={(event) => props.onChange(event.target.value)}>
        <option value="">Выберите…</option><option value="Нет">Нет</option><option value="Да">Да</option>
      </select>
    );
  } else if (kind === 'select' && !prompt.allow_custom_option) {
    control = (
      <select id={inputId} value={props.value} disabled={props.skipped} onChange={(event) => props.onChange(event.target.value)}>
        <option value="">Выберите…</option>
        {options.map((option) => <option key={option} value={option}>{option}</option>)}
      </select>
    );
  } else {
    control = (
      <>
        <input
          id={inputId}
          value={props.value}
          disabled={props.skipped}
          inputMode={kind === 'number' || kind === 'money' ? 'decimal' : undefined}
          placeholder={kind === 'date' ? 'ДД.ММ.ГГГГ' : hint || 'Введите значение'}
          list={kind === 'select' && prompt.allow_custom_option ? `${inputId}-options` : undefined}
          onChange={(event) => props.onChange(event.target.value)}
        />
        {kind === 'select' && prompt.allow_custom_option ? (
          <datalist id={`${inputId}-options`}>{options.map((option) => <option key={option} value={option} />)}</datalist>
        ) : null}
      </>
    );
  }

  return (
    <div className={`clientField ${props.skipped ? 'skipped' : ''}`}>
      <label className="clientFieldLabel" htmlFor={inputId}>{prompt.title}{prompt.required && <b>*</b>}</label>
      <div>
        <span className="clientFieldControl">
          {control}
          {hint ? <small>{hint}</small> : null}
          {prompt.required && prompt.skippable ? (
            <button
              type="button"
              className="continueWithoutValue"
              aria-pressed={props.skipped}
              onClick={() => props.onSkipChange(!props.skipped)}
            >
              {props.skipped ? 'Вернуться к заполнению' : 'Продолжить без этого значения'}
            </button>
          ) : null}
          {props.skipped ? <small className="skipWarning">Поле будет намеренно оставлено пустым только по вашему подтверждению.</small> : null}
        </span>
        {props.showPin !== false ? <button type="button" className="iconOnlyBtn" disabled={props.skipped} title="Использовать это значение во всех документах комплекта" aria-label={`Использовать ${prompt.title} во всех документах`} onClick={props.onPin}><i className="ti ti-pin" aria-hidden="true" /></button> : null}
      </div>
    </div>
  );
}
