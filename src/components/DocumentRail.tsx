import type { DocumentTemplateSpec } from '../lib/types';

interface DocumentRailProps {
  documents: DocumentTemplateSpec[];
  activeDocumentId: string | null;
  selectedDocumentIds: string[];
  busy: boolean;
  printCopies: Record<string, number>;
  onSelect(document: DocumentTemplateSpec): void;
  onToggleSelected(documentId: string): void;
  onPrintCopiesChange(documentId: string, copies: number): void;
  onSelectAll(): void;
  onClearSelected(): void;
  onGenerateSelected(): void;
  onRename(): void;
  onConfigurePopups(): void;
  onScanTemplate(): void;
  onApprove(): void;
  onRemove(): void;
  onAdd(): void;
  onToggleUtilities(): void;
}

export function DocumentRail(props: DocumentRailProps) {
  const hasDocuments = props.documents.length > 0;
  const selectedCount = props.selectedDocumentIds.length;

  return (
    <aside className="rail">
      <div className="railHead">Документы</div>
      {hasDocuments ? (
        <>
          <div className="docList" aria-label="Документы для создания">
            {props.documents.map((document) => {
              const selected = props.selectedDocumentIds.includes(document.id);
              return (
                <div
                  key={document.id}
                  className={props.activeDocumentId === document.id ? 'docRow on' : 'docRow'}
                >
                  <label className="docPick" title="Добавить документ в комплект">
                    <input
                      type="checkbox"
                      checked={selected}
                      aria-label={`Добавить ${document.button_label} в комплект`}
                      onChange={() => props.onToggleSelected(document.id)}
                    />
                  </label>
                  <button
                    className="docOpen"
                    onClick={() => props.onSelect(document)}
                  >
                    <i className="ti ti-file-text" aria-hidden="true" />
                    <span>{document.button_label}</span>
                  </button>
                  <label className="copyCount" title="Количество экземпляров этого документа при печати. 0 — не печатать.">
                    <span>копий</span>
                    <input
                      type="number"
                      min={0}
                      max={99}
                      value={props.printCopies[document.id] ?? 1}
                      aria-label={`Количество копий для ${document.button_label}`}
                      onChange={(event) => props.onPrintCopiesChange(document.id, Number(event.target.value))}
                    />
                  </label>
                </div>
              );
            })}
          </div>

          <div className="batchControls" aria-label="Управление комплектом">
            <div className="batchLinks">
              <button className="textBtn" onClick={props.onSelectAll}>Выбрать все</button>
              <button className="textBtn" onClick={props.onClearSelected} disabled={!selectedCount}>Снять</button>
            </div>
            <button
              className="primaryBtn full batchGenerate"
              onClick={props.onGenerateSelected}
              disabled={!selectedCount || props.busy}
            >
              <i className="ti ti-files" aria-hidden="true" />
              {props.busy ? 'Формирование…' : `Сформировать комплект (${selectedCount})`}
            </button>
          </div>

          {props.activeDocumentId && (
            <div className="docManage">
              <button className="primaryBtn" onClick={props.onScanTemplate}><i className="ti ti-hand-click" aria-hidden="true" /> Разметить шаблон мышью</button>
              <button className="softBtn" onClick={props.onConfigurePopups}>Настроить вопросы</button>
              <button className="softBtn" onClick={props.onRename}>Переименовать</button>
              <button className="softBtn" onClick={props.onApprove}>Утвердить ревизию формы</button>
              <button className="softBtn danger" onClick={props.onRemove}>Убрать кнопку</button>
            </div>
          )}
          <button className="docItem add" onClick={props.onAdd}>
            <i className="ti ti-plus" aria-hidden="true" />Добавить документ
          </button>
        </>
      ) : (
        <button className="primaryBtn full" onClick={props.onAdd}>Создать свои кнопки</button>
      )}
      <button className="railUtil" onClick={props.onToggleUtilities}>
        <i className="ti ti-tool" aria-hidden="true" /> Служебные сценарии
      </button>
    </aside>
  );
}
