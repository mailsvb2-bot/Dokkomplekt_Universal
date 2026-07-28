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
  const selectedCount = props.selectedDocumentIds.length;
  const hasDocuments = props.documents.length > 0;

  return (
    <aside className="packagePanel simplePackagePanel" aria-label="Документы для создания">
      <div className="packageHeader">
        <div><span>02</span><h2>Документы</h2></div>
        {hasDocuments && <span className="packageCount">{selectedCount}/{props.documents.length}</span>}
      </div>

      {!hasDocuments ? (
        <div className="emptyPackage firstRunButtons">
          <div><i className="ti ti-layout-grid-add" /></div>
          <h3>Создайте кнопки документов</h3>
          <p>Выберите ваши шаблоны Word. Один файл станет одной кнопкой.</p>
          <button className="primaryBtn full firstRunCreateButtons" onClick={props.onAdd}>Создать свои кнопки</button>
        </div>
      ) : (
        <>
          <p className="packageHint">Галочкой выберите документы для комплекта.</p>
          <div className="packageList simpleDocumentButtons">
            {props.documents.map((document) => {
              const selected = props.selectedDocumentIds.includes(document.id);
              const active = props.activeDocumentId === document.id;
              return (
                <div key={document.id} className={`packageItem ${selected ? 'selected' : ''} ${active ? 'active' : ''}`}>
                  <label className="packageCheck">
                    <input type="checkbox" checked={selected} aria-label={`Добавить ${document.button_label} в комплект`} onChange={() => props.onToggleSelected(document.id)} />
                    <span aria-hidden="true"><i className="ti ti-check" /></span>
                  </label>
                  <button className="packageOpen" onClick={() => props.onSelect(document)} aria-label={document.button_label}>
                    <i className="ti ti-file-text" aria-hidden="true" /><span>{document.button_label}</span>
                  </button>
                </div>
              );
            })}
          </div>
          <div className="packageSelectionActions">
            <button className="textBtn" onClick={props.onSelectAll}>Выбрать все</button>
            <button className="textBtn" onClick={props.onClearSelected} disabled={!selectedCount}>Снять выбор</button>
            <button className="textBtn" onClick={props.onAdd}>Добавить кнопки</button>
          </div>
          <button className="primaryBtn full packageGenerate" onClick={props.onGenerateSelected} disabled={!selectedCount || props.busy}>
            {props.busy ? 'Создаём…' : selectedCount ? `Создать комплект (${selectedCount})` : 'Выберите документы'}
          </button>
          <details className="packageSettings">
            <summary>Настройка кнопок</summary>
            <div className="packageSettingsBody">
              {props.activeDocumentId && <>
                <button className="softBtn" onClick={props.onConfigurePopups}>Уточняющие вопросы</button>
                <button className="softBtn" onClick={props.onScanTemplate}>Показать места заполнения</button>
                <button className="softBtn" onClick={props.onRename}>Переименовать</button>
                <button className="softBtn" onClick={props.onApprove}>Подтвердить версию</button>
                <button className="softBtn danger" onClick={props.onRemove}>Удалить кнопку</button>
              </>}
              <label className="checkLine compact"><input type="checkbox" checked={props.extraRulesEnabled} onChange={(event) => props.onExtraRulesChange(event.target.checked)} /><span>Дополнительные правила шаблонов</span></label>
              <details className="copySettings"><summary>Количество экземпляров</summary>{props.documents.map(document => <label key={document.id}><span>{document.button_label}</span><input type="number" min={0} max={99} value={props.printCopies[document.id] ?? 1} aria-label={`Количество копий для ${document.button_label}`} onChange={(event) => props.onPrintCopiesChange(document.id, Number(event.target.value))} /></label>)}</details>
            </div>
          </details>
        </>
      )}
      <button className="settingsLink" onClick={props.onToggleUtilities}><i className="ti ti-settings" aria-hidden="true" /> Настройки программы</button>
    </aside>
  );
}
