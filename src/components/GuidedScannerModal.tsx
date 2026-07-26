import type { GuidedScannerMarkupAction, WordScannerCapture, WordScannerSession } from '../lib/types';
import type { ScannerFieldSuggestion } from '../lib/scannerSuggestions';

interface GuidedScannerModalProps {
  mode: 'source' | 'template';
  session: WordScannerSession;
  capture: WordScannerCapture | null;
  suggestions: ScannerFieldSuggestion[];
  selectedFieldId: string;
  rememberRule: boolean;
  addQuestion: boolean;
  markupAction: GuidedScannerMarkupAction;
  busy: boolean;
  targetLabel?: string | null;
  onCapture(): void;
  onReturnToWord(): void;
  onRetry(): void;
  onSelectedFieldIdChange(value: string): void;
  onRememberRuleChange(value: boolean): void;
  onAddQuestionChange(value: boolean): void;
  onMarkupActionChange(value: GuidedScannerMarkupAction): void;
  onConfirm(): void;
  onCancel(): void;
}

export function GuidedScannerModal(props: GuidedScannerModalProps) {
  const selected = props.selectedFieldId
    ? props.suggestions.find((item) => item.field_id === props.selectedFieldId) ?? null
    : null;
  const sourceMode = props.mode === 'source';
  const confidenceLabel = selected ? confidenceText(selected.confidence) : '';

  return (
    <div className="backdrop guidedScannerBackdrop" role="dialog" aria-modal="true" aria-label="Простой сканер мышью">
      <div className="modal guidedScannerModal">
        <div className="guidedScannerTitle">
          <div className="guidedScannerIcon"><i className="ti ti-wand" aria-hidden="true" /></div>
          <div>
            <h2>Сканер мышью</h2>
            <p className="hint">Ничего настраивать и запоминать не нужно. Просто покажите программе нужное место в Word.</p>
          </div>
        </div>

        {!props.capture ? (
          <ScannerInstructions
            session={props.session}
            busy={props.busy}
            onCapture={props.onCapture}
            onReturnToWord={props.onReturnToWord}
          />
        ) : (
          <>
            <div className="capturedValueCard">
              <small>Вы показали программе</small>
              <strong>{props.capture.selected_text}</strong>
              {props.capture.expanded_from_cursor && (
                <span>Вы ничего не выделяли — программа сама взяла слово, внутри которого стоял курсор.</span>
              )}
            </div>

            {selected ? (
              <div className="recommendedField">
                <small>{confidenceLabel}</small>
                <strong>{selected.title}</strong>
                <span>{selected.reason}</span>
                <DestinationList destinations={selected.destinations} targetLabel={props.targetLabel} sourceMode={sourceMode} />
              </div>
            ) : (
              <div className="scannerWarning">Программа не смогла уверенно понять значение. Ниже можно выбрать понятный вариант.</div>
            )}

            <details className="scannerAlternatives" open={!selected}>
              <summary>Нет, это другое значение</summary>
              <div className="scannerSuggestionList" aria-label="Варианты назначения поля">
                {props.suggestions.map((suggestion) => (
                  <label className={suggestion.field_id === props.selectedFieldId ? 'scannerSuggestion selected' : 'scannerSuggestion'} key={suggestion.field_id}>
                    <input
                      type="radio"
                      name="scanner-field"
                      checked={suggestion.field_id === props.selectedFieldId}
                      onChange={() => props.onSelectedFieldIdChange(suggestion.field_id)}
                    />
                    <span><b>{suggestion.title}</b><small>{suggestion.reason}</small></span>
                    <em>{confidenceText(suggestion.confidence)}</em>
                  </label>
                ))}
              </div>

              <details className="scannerTechnicalField">
                <summary>Нужного варианта нет — создать своё поле</summary>
                <label className="scannerManualField">
                  <span>Техническое имя нового поля</span>
                  <input value={props.selectedFieldId} onChange={(event) => props.onSelectedFieldIdChange(event.target.value)} placeholder="например custom.nomer_zayavki" />
                  <small>Этот пункт нужен редко. Обычно достаточно выбрать один из вариантов выше.</small>
                </label>
              </details>
            </details>

            {sourceMode ? (
              <details className="scannerAutomationDetails" open={Boolean(props.targetLabel)}>
                <summary>Что программа сделает автоматически</summary>
                <div className="scannerOptions">
                  <div className="inlineCheck bigCheck scannerAutoLearning">
                    <i className="ti ti-brain" aria-hidden="true" />
                    <span><b>Автообучение включено</b><small>После подтверждения правило безопасно сохранится локально и будет применяться ко всем следующим похожим источникам.</small></span>
                  </div>
                  {props.targetLabel && (
                    <label className="inlineCheck bigCheck">
                      <input type="checkbox" checked={props.addQuestion} onChange={(event) => props.onAddQuestionChange(event.target.checked)} />
                      <span><b>Если значение не найдётся — спросить</b><small>Программа добавит понятный вопрос для документа «{props.targetLabel}».</small></span>
                    </label>
                  )}
                </div>
              </details>
            ) : (
              <div className="scannerTemplateDecision">
                <div className="scannerTemplateDecisionMain">
                  <i className="ti ti-sparkles" aria-hidden="true" />
                  <span>
                    <b>{props.markupAction === 'replace' ? 'Программа заменит выбранный пример на изменяемое поле.' : 'Программа оставит подпись и вставит изменяемое поле сразу после неё.'}</b>
                    <small>Исходный Word-файл останется нетронутым: меняется только безопасная копия.</small>
                  </span>
                </div>
                <details className="scannerAutomationDetails">
                  <summary>Изменить способ вставки</summary>
                  <div className="scannerOptions">
                    <label className="scannerActionChoice">
                      <input type="radio" checked={props.markupAction === 'replace'} onChange={() => props.onMarkupActionChange('replace')} />
                      <span><b>Заменить пример на поле</b><small>Для номера, даты, ФИО, суммы и другого готового значения.</small></span>
                    </label>
                    <label className="scannerActionChoice">
                      <input type="radio" checked={props.markupAction === 'insert_after'} onChange={() => props.onMarkupActionChange('insert_after')} />
                      <span><b>Оставить подпись и вставить поле после неё</b><small>Например, если выделено «Номер договора:».</small></span>
                    </label>
                    <label className="inlineCheck bigCheck">
                      <input type="checkbox" checked={props.addQuestion} onChange={(event) => props.onAddQuestionChange(event.target.checked)} />
                      <span><b>Если значение не найдётся — спросить перед созданием</b><small>Программа автоматически создаст понятный вопрос.</small></span>
                    </label>
                  </div>
                </details>
              </div>
            )}

            <div className="scannerConfirmSummary">
              <i className="ti ti-check" aria-hidden="true" />
              <span>{sourceMode ? 'После подтверждения Word закроется сам, а значение станет доступно всему комплекту.' : 'После подтверждения Word сохранит размеченную копию и закроется сам.'}</span>
            </div>
          </>
        )}

        <div className="modalActions guidedScannerActions">
          <button className="softBtn" onClick={props.onCancel} disabled={props.busy}>Отмена — всё закрыть</button>
          <span className="spacer" />
          {props.capture && (
            <>
              <button className="softBtn" onClick={props.onRetry} disabled={props.busy}>
                <i className="ti ti-refresh" aria-hidden="true" /> Выделить другое
              </button>
              <button className="primaryBtn" onClick={props.onConfirm} disabled={props.busy || !props.selectedFieldId.trim()}>
                <i className="ti ti-check" aria-hidden="true" /> Да, всё правильно
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function ScannerInstructions(props: {
  session: WordScannerSession;
  busy: boolean;
  onCapture(): void;
  onReturnToWord(): void;
}) {
  return (
    <>
      <div className="scannerSteps">
        <div className="scannerStep done"><b>1</b><span>Программа уже открыла документ в Word</span></div>
        <div className="scannerStep active"><b>2</b><span>Выделите нужное значение мышкой. Можно просто поставить курсор внутрь слова или номера.</span></div>
        <div className="scannerStep"><b>3</b><span>Вернитесь в Доккомплект и нажмите большую кнопку</span></div>
      </div>
      <div className="scannerOpenedPath" title={props.session.opened_path}>
        <i className="ti ti-file-text" aria-hidden="true" /> {fileName(props.session.opened_path)}
      </div>
      <button className="primaryBtn giantScannerButton" onClick={props.onCapture} disabled={props.busy}>
        <i className="ti ti-cursor-text" aria-hidden="true" /> Я показал значение — продолжить
      </button>
      <button className="scannerWordAgain" onClick={props.onReturnToWord} disabled={props.busy}>
        Word не видно? Открыть документ ещё раз
      </button>
      <small className="hint scannerSafety">Программа не меняет исходный документ и сама закроет Word после подтверждения или отмены.</small>
    </>
  );
}

function DestinationList(props: { destinations: string[]; targetLabel?: string | null; sourceMode: boolean }) {
  if (props.destinations.length) {
    return <div className="scannerDestinations"><b>Готовые места найдены:</b> {props.destinations.join(', ')}</div>;
  }
  if (props.targetLabel) {
    return props.sourceMode
      ? <div className="scannerDestinations suggested"><b>Предлагаемый документ:</b> {props.targetLabel}. Если в шаблоне ещё нет такого места, программа создаст вопрос и предложит разметить шаблон.</div>
      : <div className="scannerDestinations"><b>Куда попадёт:</b> в выбранное мышью место документа «{props.targetLabel}»</div>;
  }
  return <div className="scannerDestinations warning"><b>Готового места пока нет.</b> Значение сохранится для комплекта; место в шаблоне можно показать тем же сканером.</div>;
}

function confidenceText(confidence: number): string {
  if (confidence >= 0.82) return 'Почти наверняка это';
  if (confidence >= 0.62) return 'Скорее всего, это';
  return 'Возможный вариант';
}

function fileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}
