import type { DocumentTemplateSpec } from '../lib/types';

interface DocumentRailProps {
  documents: DocumentTemplateSpec[];
  activeDocumentId: string | null;
  selectedDocumentIds: string[];
  busy: boolean;
  printCopies: Record<string, number>;
  extraRulesEnabled: boolean;
  onExtraRulesChange(value: boolean): void;
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
    <aside className="packagePanel" aria-label="Состав комплекта">
      <div className="packageHeader">
        <div>
          <span>Шаг 3</span>
          <h2>Что создать</h2>
        </div>
        {hasDocuments && <span className="packageCount">{selectedCount}/{props.documents.length}</span>}
      </div>

      {hasDocuments ? (
        <>
          <p className="packageHint">Отметьте документы, которые должны войти в комплект.</p>
          <div className="packageList">
            {props.documents.map((document) => {
              const selected = props.selectedDocumentIds.includes(document.id);
              const active = props.activeDocumentId === document.id;
              return (
                <div key={document.id} className={`packageItem ${selected ? 'selected' : ''} ${active ? 'active' : ''}`}>
                  <label className="packageCheck">
                    <input
                      type="checkbox"
                      checked={selected}
                      aria-label={`Добавить ${document.button_label} в комплект`}
                      onChange={() => props.onToggleSelected(document.id)}
                    />
                    <span aria-hidden="true"><i className="ti ti-check" /></span>
                  </label>
                  <button className="packageOpen" onClick={() => props.onSelect(document)} aria-label={document.button_label}>
                    <i className="ti ti-file-text" aria-hidden="true" />
                    <span>{document.button_label}</span>
                    <small>{active ? 'открыт для проверки' : selected ? 'в комплекте' : 'не выбран'}</small>
                  </button>
                </div>
              );
            })}
          </div>

          <div className="packageSelectionActions">
            <button className="textBtn" onClick={props.onSelectAll}>Выбрать всё</button>
            <button className="textBtn" onClick={props.onClearSelected} disabled={!selectedCount}>Очистить</button>
          </div>

          <button
            className="primaryBtn full packageGenerate"
            onClick={props.onGenerateSelected}
            disabled={!selectedCount || props.busy}
          >
            <i className="ti ti-sparkles" aria-hidden="true" />
            {props.busy ? 'Создаём…' : selectedCount ? `Создать комплект (${selectedCount})` : 'Выберите документы'}
          </button>

          <details className="packageSettings">
            <summary><i className="ti ti-settings" aria-hidden="true" /> Настройка документов</summary>
            <div className="packageSettingsBody">
              <button className="softBtn" onClick={props.onAdd}><i className="ti ti-plus" aria-hidden="true" /> Добавить шаблоны</button>
              {props.activeDocumentId && (
                <>
                  <button className="softBtn" onClick={props.onConfigurePopups}>Настроить уточнения</button>
                  <button className="softBtn" onClick={props.onScanTemplate}>Разметить шаблон</button>
                  <button className="softBtn" onClick={props.onRename}>Переименовать</button>
                  <button className="softBtn" onClick={props.onApprove}>Подтвердить версию</button>
                  <button className="softBtn danger" onClick={props.onRemove}>Удалить из набора</button>
                </>
              )}
              <label className="checkLine compact"><input type="checkbox" checked={props.extraRulesEnabled} onChange={(event) => props.onExtraRulesChange(event.target.checked)} /><span>Учитывать дополнительные правила для выбранных шаблонов</span></label>
              <details className="copySettings">
                <summary>Экземпляры для печати</summary>
                {props.documents.map(document => (
                  <label key={document.id}>
                    <span>{document.button_label}</span>
                    <input type="number" min={0} max={99} value={props.printCopies[document.id] ?? 1} aria-label={`Количество копий для ${document.button_label}`} onChange={(event) => props.onPrintCopiesChange(document.id, Number(event.target.value))} />
                  </label>
                ))}
              </details>
            </div>
          </details>
        </>
      ) : (
        <div className="emptyPackage">
          <div><i className="ti ti-files" aria-hidden="true" /></div>
          <h3>Добавьте свои шаблоны</h3>
          <p>Программа превратит их в понятный набор документов и запомнит правила заполнения.</p>
          <button className="primaryBtn full" onClick={props.onAdd}>Добавить шаблоны</button>
        </div>
      )}

      <button className="settingsLink" onClick={props.onToggleUtilities}>
        <i className="ti ti-adjustments-horizontal" aria-hidden="true" /> Настройки и автоматизация
      </button>
    </aside>
  );
}
