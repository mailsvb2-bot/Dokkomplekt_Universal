import { useState, type ChangeEvent, type Dispatch, type DragEvent, type ReactNode, type SetStateAction } from 'react';
import type {
  CreatedDocumentsIntakeResult,
  GeneratedOutput,
  GeneratedPrintItem,
  IntakeCapability,
  PromptSpec,
  SemanticExtractResult,
  WorkflowPlan,
} from '../lib/types';

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
}

interface WorkspaceProps {
  busy: boolean;
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
  answers: Record<string, string>;
  preview: PreviewState | null;
  setWatchFolder(value: string): void;
  setIntakeSource(value: string): void;
  setAutoPrint(value: boolean): void;
  setSourceText(value: string): void;
  setSourceFileName(value: string | null): void;
  setWebSourceUrl(value: string): void;
  setScannerField(value: string): void;
  setScannerText(value: string): void;
  setModelOutput(value: string): void;
  setAnswers: Dispatch<SetStateAction<Record<string, string>>>;
  onRunZeroTouch(): void;
  onOpenLastOutput(): void;
  onPrintLastOutput(): void;
  onExportLastOutputPdf(): void;
  onExportLastOutputPdfa(): void;
  onExportLastOutputKedo(): void;
  onPickSourceFile(event: ChangeEvent<HTMLInputElement>): void;
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
  onSaveFields(): void;
  onGenerate(): void;
  onGenerateSelected(): void;
}

const SOURCE_ACCEPT = '.docx,.docm,.doc,.ppt,.pptx,.pdf,.jpg,.jpeg,.png,.tif,.tiff,.bmp,.webp,.xlsx,.xls,.ods,.odt,.rtf,.txt,.md,.csv,.tsv,.json,.xml,.html,.htm,.eml,.msg,.zip,.7z,.rar';

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
  return <>{text.slice(0, index)}<mark>{text.slice(index, index + needle.length)}</mark>{text.slice(index + needle.length)}</>;
}

export function Workspace(props: WorkspaceProps) {
  const [reviewFieldId, setReviewFieldId] = useState<string | null>(null);
  const reviewField = reviewFieldId
    ? props.semantic?.fields.find(field => field.field_id === reviewFieldId) ?? null
    : null;
  const reviewEvidence = reviewField?.evidence?.[0];
  const sourceReady = Boolean(props.sourceFileName || props.parsed);
  const prompts = props.plan?.prompts ?? [];
  const reviewCount = props.semantic?.fields.filter(field => field.confidence < .95).length ?? 0;

  return (
    <main className="clientWorkspace simpleWorkspace">
      <section className="workflowHero simpleHero" aria-labelledby="workflow-title">
        <div>
          <span className="workflowEyebrow">Доккомплект</span>
          <h1 id="workflow-title">Один исходник — весь комплект</h1>
          <p>Добавьте документ и нажмите одну кнопку. Программа сама проверит данные и спросит только то, без чего действительно нельзя продолжить.</p>
        </div>
        <button className="softBtn newCaseBtn" onClick={props.onResetCase} disabled={props.busy}>
          <i className="ti ti-file-plus" aria-hidden="true" /> Новый комплект
        </button>
      </section>

      {props.lastOutput && (
        <section className="resultCard" role="status" aria-label="Комплект готов">
          <div className="resultIcon"><i className="ti ti-check" aria-hidden="true" /></div>
          <div className="resultBody">
            <span className="resultEyebrow">Готово</span>
            <h2>Создано документов: {props.lastOutput.files.length}</h2>
            <p title={props.lastOutput.folder || props.lastOutput.files[0]}>{props.lastOutput.folder || props.lastOutput.files[0]}</p>
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
            <button className="primaryBtn" onClick={props.onOpenLastOutput} disabled={props.busy}><i className="ti ti-folder-open" aria-hidden="true" /> Открыть комплект</button>
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
        className={`sourceStage simpleSourceStage ${sourceReady ? 'ready' : ''}`}
        onDragOver={(event: DragEvent<HTMLElement>) => event.preventDefault()}
        onDrop={(event: DragEvent<HTMLElement>) => {
          event.preventDefault();
          const file = event.dataTransfer.files?.[0];
          if (file) props.onDropSourceFile(file);
        }}
      >
        {!sourceReady ? (
          <div className="dropHero simpleDropHero">
            <div className="dropIcon"><i className="ti ti-file-upload" aria-hidden="true" /></div>
            <h2>Перетащите сюда исходный документ</h2>
            <span>или выберите его на компьютере</span>
            <label className="primaryBtn fileBtn largeAction">
              Выбрать исходный файл
              <input type="file" accept={SOURCE_ACCEPT} onChange={props.onPickSourceFile} data-testid="source-file-input" style={{ display: 'none' }} />
            </label>
          </div>
        ) : (
          <>
            <div className="sourceAccepted simpleSourceAccepted">
              <div className="sourceFileIcon"><i className="ti ti-file-check" aria-hidden="true" /></div>
              <div className="sourceFileInfo">
                <strong>{props.sourceFileName || props.parsed?.title || 'Вставленный текст'}</strong>
                <span>{sourceKindLabel(props.parsed?.sourceKind)}{props.parsed ? ` · найдено значений: ${props.parsed.count}` : ''}</span>
                {props.parsed?.warnings.length
                  ? <em>Программа уточнит спорные данные перед созданием</em>
                  : <em className="okText">Исходник принят</em>}
              </div>
              <label className="textBtn fileBtn">
                Заменить
                <input type="file" accept={SOURCE_ACCEPT} onChange={props.onPickSourceFile} style={{ display: 'none' }} />
              </label>
            </div>

            <div className="oneClickPanel">
              <button className="primaryBtn oneClickCreate" onClick={props.onGenerateSelected} disabled={props.busy}>
                <i className="ti ti-sparkles" aria-hidden="true" />
                {props.busy ? 'Создаю комплект…' : 'Создать комплект'}
              </button>
              <p>Все выбранные кнопки документов будут сформированы и сохранены автоматически.</p>
            </div>
          </>
        )}

        <details className="alternativeSource simpleOptional">
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

      {(props.plan || props.preview) && (
        <details className="reviewStage simpleOptional" open>
          <summary>Проверка отдельного документа</summary>
          {prompts.length ? (
            <div className="clientFields">
              {prompts.map((prompt: PromptSpec) => (
                <label className="clientField" key={prompt.field_id}>
                  <span>{prompt.title}{prompt.required && <b>*</b>}</span>
                  <div>
                    <input
                      value={props.answers[prompt.field_id] ?? ''}
                      placeholder={prompt.validation_hint || 'Введите значение'}
                      onChange={(event) => props.setAnswers((previous) => ({ ...previous, [prompt.field_id]: event.target.value }))}
                    />
                    <button className="iconOnlyBtn" title="Использовать это значение во всех документах комплекта" aria-label={`Использовать ${prompt.title} во всех документах`} onClick={() => props.onPinField(prompt.field_id)}><i className="ti ti-pin" aria-hidden="true" /></button>
                  </div>
                </label>
              ))}
              <div className="reviewActions">
                <button className="primaryBtn" onClick={props.onSaveFields} disabled={props.busy}>Сохранить ответы</button>
                <button className="softBtn" onClick={props.onPreview} disabled={props.busy}>Предпросмотр</button>
              </div>
            </div>
          ) : (
            <div className="readyMessage">
              <i className="ti ti-circle-check" aria-hidden="true" />
              <div><strong>Данные готовы</strong><span>{props.semantic ? `Распознано значений: ${props.semantic.fields.length}${reviewCount ? ` · рекомендуем проверить: ${reviewCount}` : ''}` : 'Проверка будет выполнена перед сохранением.'}</span></div>
              <button className="softBtn" onClick={props.onUnderstand} disabled={props.busy}>Проверить данные</button>
            </div>
          )}
          {props.preview && (
            <div className="clientPreview">
              <strong>Предпросмотр{props.preview.missing ? ` · не заполнено: ${props.preview.missing}` : ''}</strong>
              <pre>{props.preview.text || '—'}</pre>
              <button className="primaryBtn" onClick={props.onGenerate} disabled={props.busy}>Создать только этот документ</button>
            </div>
          )}
        </details>
      )}

      <details className="automationCard simpleOptional">
        <summary>
          <span className="automationIcon"><i className="ti ti-bolt" aria-hidden="true" /></span>
          <span><strong>Автоматическая обработка папки</strong><small>Для работы без ручного запуска.</small></span>
          <i className="ti ti-chevron-down" aria-hidden="true" />
        </summary>
        <div className="automationBody">
          <label><span>Рабочая папка</span><input value={props.watchFolder} onChange={(event) => props.setWatchFolder(event.target.value)} placeholder="Созданные документы" /></label>
          <label><span>Обработать файл по пути</span><div className="inlineInput"><input value={props.intakeSource} onChange={(event) => props.setIntakeSource(event.target.value)} placeholder="Путь к файлу" /><button className="primaryBtn" onClick={props.onRunZeroTouch} disabled={props.busy}>Обработать указанный файл</button></div></label>
          <label className="checkLine"><input type="checkbox" checked={props.autoPrint} onChange={(event) => props.setAutoPrint(event.target.checked)} /><span>Печатать готовый комплект автоматически</span></label>
          <small className="automationHelp">Если файл временно нельзя прочитать, рядом появится заметка «НЕ ПРОЧИТАН.txt» с понятной причиной и временем следующей попытки.</small>
          {props.intakeResult && <div className={`automationResult ${props.intakeResult.status}`}><strong>{props.intakeResult.status === 'processed' ? 'Комплект создан' : props.intakeResult.status === 'attention' ? 'Нужно уточнение' : 'Информация'}</strong><span>{props.intakeResult.message}</span></div>}
        </div>
      </details>

      <details className="advancedCard simpleOptional">
        <summary><i className="ti ti-adjustments" aria-hidden="true" /> Расширенные инструменты</summary>
        <div className="advancedBody">
          <section>
            <h3>Точность распознавания</h3>
            <p>Открывайте этот раздел только когда программа ошиблась или не нашла значение.</p>
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
