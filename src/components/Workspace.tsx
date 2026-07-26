import { useState, type ChangeEvent, type Dispatch, type DragEvent, type ReactNode, type SetStateAction } from 'react';
import type {
  CreatedDocumentsIntakeResult,
  GeneratedOutput,
  Icd10Suggestion,
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
  icdQuery: string;
  icdHits: Icd10Suggestion[];
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
  setIcdQuery(value: string): void;
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
  onSearchDictionary(): void;
  onChooseDictionaryValue(value: Icd10Suggestion): void;
  onGenerate(): void;
}

function sourceKindLabel(kind?: string): string {
  switch (kind) {
    case 'scanned_image': return 'скан/фото · OCR';
    case 'scanned_pdf_ocr': return 'скан-PDF · OCR';
    case 'mixed_pdf_page_ocr': return 'смешанный PDF · постраничный OCR';
    case 'pdf_text': return 'PDF с текстовым слоем';
    case 'word': return 'Word';
    case 'legacy_word_converted': return 'старый DOC · локально преобразован';
    case 'presentation_converted': return 'презентация · локально преобразована';
    case 'spreadsheet': return 'таблица';
    case 'archive': return 'архив';
    case 'email': return 'письмо';
    case 'https': return 'HTTPS';
    case 'manual_text': return 'вставленный текст';
    default: return kind || 'источник';
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
  return (
    <>
      <main className="mid">
        <section className="block zeroTouch">
          <div className="blockHead"><i className="ti ti-folder-bolt" aria-hidden="true" /> Созданные документы — автоматический комплект</div>
          <p className="hint">Специалист кладёт исходный документ, письмо, таблицу, изображение или архив в эту папку — программа нормализует источник и строит весь настроенный комплект в отдельной обезличенной папке. Не хватает данных — рядом появляется «…_ТРЕБУЕТ_ВНИМАНИЯ.txt». Файл повреждён или не читается — появляется «… — НЕ ПРОЧИТАН.txt» с простыми шагами исправления. До изменения источника программа не повторяет попытку бесконечно. Одна версия источника обрабатывается один раз.</p>
          {props.lastOutput && (
            <div className="completionCard" role="status">
              <div className="completionTitle"><i className="ti ti-circle-check" aria-hidden="true" /> Комплект готов: {props.lastOutput.files.length} документ(ов)</div>
              <div className="completionPath">{props.lastOutput.folder || props.lastOutput.files[0]}</div>
              {props.lastOutput.print_items?.length ? (
                <div className="printCopyList" aria-label="Количество копий по документам">
                  {props.lastOutput.print_items.map((item: GeneratedPrintItem) => (
                    <label key={`${item.document_id}:${item.path}`} className="printCopyRow">
                      <span title={item.path}>{item.label}</span>
                      <input
                        type="number"
                        min={0}
                        max={99}
                        value={props.printCopies[item.document_id] ?? 1}
                        aria-label={`Количество копий для ${item.label}`}
                        onChange={(event) => props.onPrintCopyChange(item.document_id, Number(event.target.value))}
                      />
                      <small>экз.</small>
                    </label>
                  ))}
                </div>
              ) : null}
              <div className="completionActions">
                <button className="softBtn" onClick={props.onOpenLastOutput} disabled={props.busy}><i className="ti ti-folder-open" aria-hidden="true" /> Открыть папку</button>
                <button className="softBtn" onClick={props.onExportLastOutputPdf} disabled={props.busy}><i className="ti ti-file-type-pdf" aria-hidden="true" /> Создать PDF</button>
                <button className="softBtn" onClick={props.onExportLastOutputPdfa} disabled={props.busy}><i className="ti ti-archive" aria-hidden="true" /> Архивный PDF/A-1</button>
                <button className="softBtn" onClick={props.onExportLastOutputKedo} disabled={props.busy}><i className="ti ti-package-export" aria-hidden="true" /> КЭДО-пакет</button>
                <button className="primaryBtn" onClick={props.onPrintLastOutput} disabled={props.busy}><i className="ti ti-printer" aria-hidden="true" /> Распечатать выбранное количество</button>
              </div>
            </div>
          )}
          <div className="ztRow">
            <i className="ti ti-folder" aria-hidden="true" />
            <input value={props.watchFolder} onChange={(event) => props.setWatchFolder(event.target.value)} placeholder="папка «Созданные документы»" aria-label="Папка Созданные документы" />
          </div>
          <div className="ztRow">
            <i className="ti ti-file-import" aria-hidden="true" />
            <input value={props.intakeSource} onChange={(event) => props.setIntakeSource(event.target.value)} placeholder="путь к исходному документу: Word, PDF, фото, таблица, письмо или архив" aria-label="Исходный документ" />
            <button className="primaryBtn" onClick={props.onRunZeroTouch} disabled={props.busy}><i className="ti ti-bolt" aria-hidden="true" /> Обработать источник</button>
          </div>
          <label className="autoPrintToggle">
            <input type="checkbox" checked={props.autoPrint} onChange={(event) => props.setAutoPrint(event.target.checked)} />
            <span><i className="ti ti-printer" aria-hidden="true" /> Печатать готовый комплект автоматически без вопроса</span>
          </label>
          {props.intakeResult && (
            <div className={`ztResult ${props.intakeResult.status}`}>
              {props.intakeResult.status === 'processed' && (
                <>
                  <div className="ztLine ok"><i className="ti ti-check" aria-hidden="true" /> Комплект создан: {props.intakeResult.created_files.length} документ(ов)</div>
                  {props.intakeResult.patient_folder && <div className="ztPath">{props.intakeResult.patient_folder}</div>}
                </>
              )}
              {props.intakeResult.status === 'attention' && (
                <>
                  <div className="ztLine warn"><i className="ti ti-alert-triangle" aria-hidden="true" /> Не хватает данных — ничего не создано</div>
                  <ul className="ztMissing">{props.intakeResult.missing.map((item) => <li key={item}>{item}</li>)}</ul>
                  {props.intakeResult.attention_file && <div className="ztPath">{props.intakeResult.attention_file}</div>}
                </>
              )}
              {(props.intakeResult.status === 'setup_needed' || props.intakeResult.status === 'ignored') && (
                <div className="ztLine"><i className="ti ti-info-circle" aria-hidden="true" /> {props.intakeResult.message}</div>
              )}
            </div>
          )}
        </section>

        <section
          className="block fileDropZone"
          onDragOver={(event: DragEvent<HTMLElement>) => event.preventDefault()}
          onDrop={(event: DragEvent<HTMLElement>) => {
            event.preventDefault();
            const file = event.dataTransfer.files?.[0];
            if (file) props.onDropSourceFile(file);
          }}
        >
          <div className="blockHead"><i className="ti ti-scan" aria-hidden="true" /> Источник — распознавание</div>
          <div className="sourceToolbar">
            <button className="softBtn" onClick={props.onResetCase} disabled={props.busy}>
              <i className="ti ti-file-plus" aria-hidden="true" /> Новый комплект
            </button>
            <label className="softBtn fileBtn">
              <i className="ti ti-file-upload" aria-hidden="true" /> Выбрать исходный файл
              <input type="file" accept=".docx,.docm,.doc,.ppt,.pptx,.pdf,.jpg,.jpeg,.png,.tif,.tiff,.bmp,.webp,.xlsx,.xls,.ods,.odt,.rtf,.txt,.md,.csv,.tsv,.json,.xml,.html,.htm,.eml,.msg,.zip,.7z,.rar" onChange={props.onPickSourceFile} data-testid="source-file-input" style={{ display: 'none' }} />
            </label>
            <span className="hint">{props.sourceFileName ? `Выбран: ${props.sourceFileName}` : 'Перетащите Word/PDF/фото/таблицу/письмо/архив, выберите файл, загрузите HTTPS-источник или вставьте текст.'}</span>
          </div>
          <div className="ztRow webSourceRow">
            <i className="ti ti-world" aria-hidden="true" />
            <input
              value={props.webSourceUrl}
              onChange={(event) => props.setWebSourceUrl(event.target.value)}
              placeholder="https://сайт.example/документ или JSON/XML/CSV API"
              aria-label="HTTPS-источник"
            />
            <button className="softBtn" onClick={props.onLoadWebSource} disabled={props.busy || !props.webSourceUrl.trim()}>
              <i className="ti ti-download" aria-hidden="true" /> Загрузить HTTPS
            </button>
          </div>
          {props.intakeCapabilities.length > 0 && (
            <details className="intakeCapabilities">
              <summary>Готовность форматов и внешних движков</summary>
              <ul>
                {props.intakeCapabilities.map((item) => (
                  <li key={item.format} className={item.ready ? 'ready' : 'missing'}>
                    <strong>{item.format}</strong> · {item.extensions.join(', ')} · {item.ready ? 'готово' : 'нужен компонент'} · {item.mode}
                    <small>{item.detail}</small>
                  </li>
                ))}
              </ul>
            </details>
          )}
          <div className="guidedScannerLaunch">
            <div className="guidedScannerLaunchText">
              <span className="guidedScannerEyebrow">Самый простой способ</span>
              <strong>Покажите значение прямо в Word — остальное программа сделает сама</strong>
              <small>1. Программа откроет документ. 2. Вы выделите нужное мышкой или поставите курсор внутрь слова. 3. Программа сама предложит, что это и в какие документы подставить, затем сама закроет Word.</small>
            </div>
            <button className="primaryBtn guidedScannerLaunchButton" onClick={props.onStartGuidedSourceScanner} disabled={props.busy || !props.sourceFilePath || !/\.doc[xm]$/i.test(props.sourceFileName ?? '')}>
              <i className="ti ti-hand-click" aria-hidden="true" /> Открыть Word и показать значение мышкой
            </button>
            {(!props.sourceFilePath || !/\.doc[xm]$/i.test(props.sourceFileName ?? '')) && <small className="guidedScannerDisabledHint">Разметка мышкой в Word доступна только для DOCX/DOCM; остальные форматы сначала автоматически распознаются.</small>}
          </div>
          <textarea
            className="source"
            value={props.sourceText}
            onChange={(event) => {
              props.setSourceText(event.target.value);
              props.setSourceFileName(null);
            }}
            onSelect={(event) => {
              const target = event.currentTarget;
              const start = target.selectionStart ?? 0;
              const end = target.selectionEnd ?? start;
              if (end > start) props.setScannerText(target.value.slice(start, end));
            }}
            spellCheck={false}
          />
          <details className="manualScannerDetails">
            <summary>Ручной режим для опытных пользователей</summary>
            <div className="visualScannerBar">
              <div className="scannerSelection" title={props.scannerText || 'Выделите текст мышкой в документе'}>
                <i className="ti ti-cursor-text" aria-hidden="true" />
                {props.scannerText ? `Выделено: ${props.scannerText}` : 'Выделите фрагмент мышкой в тексте источника'}
              </div>
              <input
                value={props.scannerField}
                onChange={(event) => props.setScannerField(event.target.value)}
                placeholder="идентификатор поля: document.number"
                aria-label="Поле для выделенного фрагмента"
                list="scanner-field-suggestions"
              />
              <datalist id="scanner-field-suggestions">
                <option value="document.number" />
                <option value="document.date" />
                <option value="subject.name" />
                <option value="org.name" />
                <option value="period.start_date" />
                <option value="period.end_date" />
                <option value="medical.case_number" />
                <option value="medical.diagnosis" />
                <option value="medical.treatment" />
              </datalist>
              <button className="softBtn" onClick={props.onApplyScannerSelection} disabled={props.busy || !props.scannerText.trim() || !props.scannerField.trim()}>
                <i className="ti ti-color-swatch" aria-hidden="true" /> Назначить выделение полю
              </button>
              <button className="softBtn" onClick={props.onApplyScannerAndQuestion} disabled={props.busy || !props.scannerText.trim() || !props.scannerField.trim()}>
                <i className="ti ti-message-plus" aria-hidden="true" /> Назначить и добавить вопрос
              </button>
            </div>
          </details>
          <div className="rowBetween">
            <span className="hint">Извлекаем поля из вашего документа и сразу подключаем их к сценариям и генерации.</span>
            <button className="primaryBtn" onClick={props.onParseSource} disabled={props.busy}><i className="ti ti-wand" aria-hidden="true" /> Разобрать текст</button>
          </div>
          {props.parsed && (
            <div className="parsedNote">
              <span className="badgeOk"><i className="ti ti-check" aria-hidden="true" /> {props.parsed.title}</span>
              <span>извлечено полей: {props.parsed.count}</span>
              <span>источник: {sourceKindLabel(props.parsed.sourceKind)}</span>
              {!!props.parsed.layoutRows && <span>структурных строк: {props.parsed.layoutRows}</span>}
              {!!props.parsed.tableRows && <span>табличных строк: {props.parsed.tableRows}</span>}
              {props.parsed.warnings.length > 0 && <span className="badgeWarn"><i className="ti ti-alert-triangle" aria-hidden="true" /> {props.parsed.warnings.length}</span>}
            </div>
          )}
        </section>

        <section className="block">
          <div className="blockHead"><i className="ti ti-brain" aria-hidden="true" /> Извлечение полей из документа</div>
          <div className="rowBetween">
            <span className="hint">Детерминированный парсер работает всегда. Дополнительно можно включить локальную Ollama/llama.cpp в центре автоматизации: документ остаётся на компьютере, модель только предлагает факты, а Rust-валидаторы решают, можно ли их использовать.</span>
            <button className="primaryBtn" onClick={props.onUnderstand} disabled={props.busy}><i className="ti ti-brain" aria-hidden="true" /> Извлечь поля</button>
          </div>
          <details className="modelDetails">
            <summary>Диагностический JSON модели — вручную</summary>
            <textarea className="source" value={props.modelOutput} onChange={(event) => props.setModelOutput(event.target.value)} placeholder={'Вставьте JSON, полученный от отдельно подключённой модели: {"org.inn": {"value": "…", "confidence": 0.9}}'} spellCheck={false} />
          </details>
          {props.semantic && (
            <div className="semanticResult">
              <div className="semHead">Извлечено полей: {props.semantic.fields.length}{props.semantic.model_applied ? ' · локальная/внешняя модель учтена' : ' · детерминированно'} · безопасных {props.semantic.fields.filter(field => field.confidence >= .95).length} · проверить {props.semantic.fields.filter(field => field.confidence < .95).length}</div>
              <ul className="semFields">
                {props.semantic.fields.map((field) => (
                  <li key={field.field_id} className={field.confidence < .8 ? 'semRisk' : field.confidence < .95 ? 'semReview' : 'semTrusted'}>
                    <span className="semId">{field.field_id}</span>
                    <span className="semVal">{field.value}</span>
                    <span className="semMeta">{field.confidence < .8 ? 'обязательно проверить' : field.confidence < .95 ? 'проверить' : 'высокая уверенность'} · {field.method} · {field.source || 'источник не указан'} · {(field.confidence * 100).toFixed(0)}%</span>
                    {!!field.evidence?.length && <details className="semEvidence"><summary>Показать доказательство</summary>{field.evidence.map((excerpt, index) => <blockquote key={`${field.field_id}-${index}`}>{excerpt}</blockquote>)}</details>}
                    <div className="semanticFieldActions">
                      <button className="softBtn" onClick={() => setReviewFieldId(field.field_id)}>Сверить источник → результат</button>
                      <button className="softBtn semCorrection" disabled={props.busy || !props.sourceFilePath} onClick={() => props.onReportSemanticError(field.field_id, field.value)}>Здесь ошибка</button>
                    </div>
                  </li>
                ))}
              </ul>
              {props.semantic.warnings.length > 0 && <div className="semWarn"><i className="ti ti-alert-triangle" aria-hidden="true" /> {props.semantic.warnings.join('; ')}</div>}
              {reviewField && <section className="evidenceReview" aria-label="Проверка источника и результата">
                <div className="evidencePane">
                  <div className="evidencePaneHead">Источник · доказательство для {reviewField.field_id}</div>
                  <pre>{highlightedSource(props.sourceText, reviewEvidence)}</pre>
                  {reviewEvidence && <small>Подсвечен первый подтверждающий фрагмент. Остальные доказательства доступны в карточке поля.</small>}
                </div>
                <div className="evidencePane">
                  <div className="evidencePaneHead">Результат · значение и готовые файлы</div>
                  <dl className="evidenceValue">
                    <dt>{reviewField.field_id}</dt><dd>{reviewField.value}</dd>
                    <dt>Уверенность</dt><dd>{(reviewField.confidence * 100).toFixed(1)}%</dd>
                    <dt>Метод</dt><dd>{reviewField.method}</dd>
                  </dl>
                  {props.preview?.text && <details><summary>Текст предпросмотра документа</summary><pre>{props.preview.text}</pre></details>}
                  {props.lastOutput?.files?.length ? <ul className="evidenceOutputFiles">{props.lastOutput.files.map(path => <li key={path}>{path}</li>)}</ul> : <small>После генерации здесь появятся пути готовых документов.</small>}
                </div>
              </section>}
            </div>
          )}
        </section>

        <section className="block">
          <div className="blockHead"><i className="ti ti-list-details" aria-hidden="true" /> Извлечённые поля{props.plan ? ` · нужно уточнить: ${props.plan.prompts.length}` : ''}</div>
          {props.plan?.prompts.length ? (
            <div className="fields">
              {props.plan.prompts.map((prompt: PromptSpec) => (
                <div className="fieldRow" key={prompt.field_id}>
                  <div className="fieldLabel">{prompt.title}{prompt.required && <span className="req">*</span>}</div>
                  <div className="fieldInputWrap">
                    <input
                      value={props.answers[prompt.field_id] ?? ''}
                      placeholder={prompt.field_id}
                      onChange={(event) => props.setAnswers((previous) => ({ ...previous, [prompt.field_id]: event.target.value }))}
                    />
                    <button className="pin" aria-label="Закрепить значение" onClick={() => props.onPinField(prompt.field_id)}><i className="ti ti-pin" aria-hidden="true" /></button>
                  </div>
                </div>
              ))}
              <div className="fieldActions">
                <button className="softBtn" onClick={props.onPreview}><i className="ti ti-eye" aria-hidden="true" /> Предпросмотр</button>
                <button className="primaryBtn" onClick={props.onSaveFields}><i className="ti ti-device-floppy" aria-hidden="true" /> Сохранить поля</button>
              </div>
            </div>
          ) : (
            <p className="hint">Выберите документ слева — покажем объединённый список полей, которые нужно уточнить.</p>
          )}

          <div className="dictBar">
            <span className="dictLabel"><i className="ti ti-book" aria-hidden="true" /> Словарь профиля</span>
            <input value={props.icdQuery} placeholder="код или значение (например, F32 / ИНН)" onChange={(event) => props.setIcdQuery(event.target.value)} />
            <button className="softBtn" onClick={props.onSearchDictionary}>Найти</button>
          </div>
          {props.icdHits.length > 0 && (
            <div className="chips">
              {props.icdHits.map((hit) => (
                <button key={hit.code} className="chip" onClick={() => props.onChooseDictionaryValue(hit)}>{hit.code} — {hit.title}</button>
              ))}
            </div>
          )}
        </section>
      </main>

      <aside className="prev">
        <div className="railHead">Предпросмотр</div>
        <div className="paper">
          {props.preview ? <pre className="paperText">{props.preview.text || '—'}</pre> : (
            <div className="paperSkeleton">
              <span className="ln title" /><span className="ln" style={{ width: '100%' }} /><span className="ln" style={{ width: '92%' }} />
              <span className="ln" style={{ width: '96%' }} /><span className="ln hl" style={{ width: '54%' }} /><span className="ln" style={{ width: '80%' }} />
            </div>
          )}
        </div>
        <div className="prevStat">
          {props.preview
            ? (props.preview.missing ? <><i className="ti ti-alert-triangle" aria-hidden="true" /> не заполнено: {props.preview.missing}</> : <><i className="ti ti-check" aria-hidden="true" /> плейсхолдеры заполнены</>)
            : <>нажмите «Предпросмотр»</>}
        </div>
        <button className="primaryBtn full" onClick={props.onGenerate} disabled={props.busy}><i className="ti ti-file-download" aria-hidden="true" /> Сформировать DOCX</button>
      </aside>
    </>
  );
}
