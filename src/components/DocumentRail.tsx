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
    <aside className="packagePanel simplePackagePanel" aria-label="Состав комплекта">
      <div className="packageHeader">
        <div>
          <span>02</span>
          <h2>Ваш комплект</h2>
        </div>
        {hasDocuments && <span className="packageCount">{selectedCount || props.documents.length}/{props.documents.length}</span>}
      </div>

      {hasDocuments ? (
        <>
          <div className="simplePackageNotice">
            <strong>Готово к работе</strong>
            <span>По умолчанию создаётся весь комплект. Ненужный документ можно выключить одним нажатием.</span>
          </div>

          <div className="packageList simpleDocumentButtons compactDocumentButtons">
            {props.documents.map((document) => {
              const selected = selectedCount === 0 || props.selectedDocumentIds.includes(document.id);
              const active = props.activeDocumentId === document.id;
              return (
                <button
                  type="button"
                  key={document.id}
                  className={`packageItem ${selected ? 'selected' : ''} ${active ? 'active' : ''}`}
                  aria-label={document.button_label}
                  aria-pressed={selected}
                  onClick={() => {
                    props.onSelect(document);
                    props.onToggleSelected(document.id);
                  }}
                >
                  <span className="packageTileIcon" aria-hidden="true"><i className="ti ti-file-text" /></span>
                  <span className="packageTileText">
                    <strong>{document.button_label}</strong>
                    <small>{selected ? 'Будет создан' : 'Не создавать'}</small>
                  </span>
                  <span className="packageTileState" aria-hidden="true"><i className={selected ? 'ti ti-check' : 'ti ti-minus'} /></span>
                </button>
              );
            })}
          </div>

          <div className="packageSelectionActions">
            <button className="textBtn" onClick={props.onSelectAll}>Весь комплект</button>
            <button className="textBtn" onClick={props.onClearSelected}>Снять выбор</button>
          </div>

          <details className="packageSettings">
            <summary><i className="ti ti-settings" aria-hidden="true" /> Управление кнопками</summary>
            <div className="packageSettingsBody">
              <button className="softBtn" onClick={props.onAdd}><i className="ti ti-plus" aria-hidden="true" /> Добавить шаблоны</button>
              {props.activeDocumentId && (
                <>
                  <button className="softBtn" onClick={props.onConfigurePopups}>Настроить уточнения</button>
                  <button className="softBtn" onClick={props.onScanTemplate}>Разметить шаблон</button>
                  <button className="softBtn" onClick={props.onRename}>Переименовать</button>
                  <button className="softBtn" onClick={props.onApprove}>Подтвердить версию</button>
                  <button className="softBtn danger" onClick={props.onRemove}>Убрать из набора</button>
                </>
              )}
              <label className="checkLine compact"><input type="checkbox" checked={props.extraRulesEnabled} onChange={(event) => props.onExtraRulesChange(event.target.checked)} /><span>Учитывать дополнительные правила выбранных шаблонов</span></label>
              <details className="copySettings">
                <summary>Количество экземпляров</summary>
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
        <div className="emptyPackage firstRunButtons">
          <div><i className="ti ti-files" /></div>
          <h3>Сначала создайте свои кнопки</h3>
          <p>Выберите используемые вами шаблоны Word. Каждый шаблон сразу станет кнопкой документа.</p>
          <button className="primaryBtn full firstRunCreateButtons" onClick={props.onAdd}>Создать свои кнопки</button>
        </div>
      )}

      {hasDocuments && (
        <button className="settingsLink" onClick={props.onToggleUtilities}>
          <i className="ti ti-adjustments-horizontal" aria-hidden="true" /> Дополнительные настройки
        </button>
      )}
    </aside>
  );
}
