import type { DocumentTemplateSpec, FolderNamePartDto, SemanticCase } from '../lib/types';
import { AdvancedToolsPanel } from './AdvancedToolsPanel';
import { AutomationControlCenter } from './AutomationControlCenter';
import { BusinessRegistryPanel } from './BusinessRegistryPanel';
import { OrganizationKnowledgePanel } from './OrganizationKnowledgePanel';

interface UtilityPanelProps {
  documents: DocumentTemplateSpec[];
  selectedDocumentIds: string[];
  onStatus(message: string): void;
  onDocumentsChanged(documents: DocumentTemplateSpec[]): void;
  seriesStart: string;
  seriesEnd: string;
  seriesSkipWeekends: boolean;
  scannerField: string;
  scannerText: string;
  outputRoot: string;
  folderParts: FolderNamePartDto[];
  licenseText: string;
  onSeriesStartChange(value: string): void;
  onSeriesEndChange(value: string): void;
  onSeriesSkipWeekendsChange(value: boolean): void;
  onScannerFieldChange(value: string): void;
  onScannerTextChange(value: string): void;
  onOutputRootChange(value: string): void;
  onFolderPartsChange(parts: FolderNamePartDto[]): void;
  onLicenseTextChange(value: string): void;
  onSeriesPlan(): void;
  onScanMarks(): void;
  onOutputPlan(): void;
  onSaveSession(): void;
  onLoadSession(): void;
  onCheckAccess(): void;
  onCheckUpdates(): void;
  onInstallWatcher(): void;
  onUninstallWatcher(): void;
  onVerifyLicense(): void;
  onSemanticCaseChanged?(semanticCase: SemanticCase): void;
}

const FOLDER_PART_OPTIONS: Array<{ value: FolderNamePartDto; label: string; sensitive?: boolean }> = [
  { value: 'DocumentNumber', label: 'номер документа' },
  { value: 'DocumentDate', label: 'дата документа' },
  { value: 'OrganizationName', label: 'организация' },
  { value: 'ShortInitials', label: 'фамилия и инициалы', sensitive: true },
  { value: 'FullSubjectName', label: 'полное имя', sensitive: true },
  { value: 'PeriodRange', label: 'период целиком' },
  { value: 'PeriodStartDate', label: 'начало периода' },
  { value: 'PeriodEndDate', label: 'окончание периода' },
];

export function UtilityPanel(props: UtilityPanelProps) {
  function toggleFolderPart(part: FolderNamePartDto, checked: boolean) {
    const next = checked
      ? [...new Set([...props.folderParts, part])]
      : props.folderParts.filter((value) => value !== part);
    props.onFolderPartsChange(next);
  }

  return (
    <section className="utilityGrid" aria-label="Дополнительные инструменты">
      <div className="utilityCard">
        <strong>Серия записей</strong>
        <input value={props.seriesStart} onChange={(event) => props.onSeriesStartChange(event.target.value)} placeholder="дата начала" />
        <input value={props.seriesEnd} onChange={(event) => props.onSeriesEndChange(event.target.value)} placeholder="дата окончания" />
        <label>
          <input
            type="checkbox"
            checked={props.seriesSkipWeekends}
            onChange={(event) => props.onSeriesSkipWeekendsChange(event.target.checked)}
          />{' '}
          пропускать выходные
        </label>
        <button className="utilBtn" onClick={props.onSeriesPlan}>
          <i className="ti ti-calendar" aria-hidden="true" /> Рассчитать
        </button>
      </div>
      <div className="utilityCard">
        <strong>Ручная разметка</strong>
        <input
          value={props.scannerField}
          onChange={(event) => props.onScannerFieldChange(event.target.value)}
          placeholder="поле, например document.number"
        />
        <input
          value={props.scannerText}
          onChange={(event) => props.onScannerTextChange(event.target.value)}
          placeholder="выделенный текст"
        />
        <button className="utilBtn" onClick={props.onScanMarks}>
          <i className="ti ti-scan" aria-hidden="true" /> Применить разметку
        </button>
      </div>
      <div className="utilityCard outputSettingsCard">
        <strong>План вывода</strong>
        <input
          value={props.outputRoot}
          onChange={(event) => props.onOutputRootChange(event.target.value)}
          placeholder="корневая папка"
        />
        <fieldset className="folderParts">
          <legend>Имя папки результата</legend>
          {FOLDER_PART_OPTIONS.map((option) => (
            <label key={option.value}>
              <input
                type="checkbox"
                checked={props.folderParts.includes(option.value)}
                onChange={(event) => toggleFolderPart(option.value, event.target.checked)}
              />
              {option.label}{option.sensitive ? ' · персональные данные' : ''}
            </label>
          ))}
        </fieldset>
        <small>Безопасное значение по умолчанию: номер и дата документа. Персональные данные включаются только явно.</small>
        <button className="utilBtn" onClick={props.onOutputPlan}>
          <i className="ti ti-folder" aria-hidden="true" /> Проверить путь
        </button>
      </div>
      <button className="utilBtn" onClick={props.onSaveSession}>
        <i className="ti ti-database-export" aria-hidden="true" /> Сохранить сессию
      </button>
      <button className="utilBtn" onClick={props.onLoadSession}>
        <i className="ti ti-database-import" aria-hidden="true" /> Загрузить сессию
      </button>
      <button className="utilBtn" onClick={props.onCheckAccess}>
        <i className="ti ti-shield-check" aria-hidden="true" /> Проверить доступ
      </button>
      <button className="utilBtn" onClick={props.onCheckUpdates}>
        <i className="ti ti-refresh" aria-hidden="true" /> Проверить обновления
      </button>
      <button className="utilBtn" onClick={props.onInstallWatcher}>
        <i className="ti ti-eye-cog" aria-hidden="true" /> Фоновый агент
      </button>
      <button className="utilBtn" onClick={props.onUninstallWatcher}>
        <i className="ti ti-eye-off" aria-hidden="true" /> Отключить агент
      </button>
      <BusinessRegistryPanel outputRoot={props.outputRoot} onStatus={props.onStatus} onCaseChanged={(semanticCase) => props.onSemanticCaseChanged?.(semanticCase)} />
      <OrganizationKnowledgePanel onStatus={props.onStatus} onCaseChanged={(semanticCase) => props.onSemanticCaseChanged?.(semanticCase)} />
      <AdvancedToolsPanel documents={props.documents} selectedDocumentIds={props.selectedDocumentIds} outputRoot={props.outputRoot} onStatus={props.onStatus} onDocumentsChanged={props.onDocumentsChanged} />
      <AutomationControlCenter onStatus={props.onStatus} />
      <div className="licenseRow">
        <input
          value={props.licenseText}
          placeholder="вставьте подписанную лицензию"
          onChange={(event) => props.onLicenseTextChange(event.target.value)}
        />
        <button className="softBtn" onClick={props.onVerifyLicense}>Активировать лицензию</button>
      </div>
    </section>
  );
}
