import type { DocumentTemplateSpec, FolderNamePartDto, SemanticCase } from '../lib/types';
import { AdvancedToolsPanel } from './AdvancedToolsPanel';
import { AutomationControlCenter } from './AutomationControlCenter';
import { BusinessRegistryPanel } from './BusinessRegistryPanel';
import { OrganizationKnowledgePanel } from './OrganizationKnowledgePanel';
import { LearningGovernancePanel } from './LearningGovernancePanel';

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
  savedOutputRoot: string;
  folderParts: FolderNamePartDto[];
  licenseText: string;
  onSeriesStartChange(value: string): void;
  onSeriesEndChange(value: string): void;
  onSeriesSkipWeekendsChange(value: boolean): void;
  onScannerFieldChange(value: string): void;
  onScannerTextChange(value: string): void;
  onOutputRootChange(value: string): void;
  onPickOutputFolder(): void;
  onSaveOutputFolder(): void;
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
  { value: 'SurnameGivenName', label: 'фамилия и имя', sensitive: true },
  { value: 'FullSubjectName', label: 'полное имя', sensitive: true },
  { value: 'PeriodRange', label: 'период целиком' },
  { value: 'PeriodStartDate', label: 'начало периода' },
  { value: 'PeriodEndDate', label: 'окончание периода' },
  { value: 'PeriodStartMonth', label: 'месяц начала периода' },
  { value: 'PeriodEndMonth', label: 'месяц окончания периода' },
  { value: 'ShortPeriodRange', label: 'период целиком · короткие даты' },
  { value: 'ShortPeriodStartDate', label: 'начало периода · короткая дата' },
  { value: 'ShortPeriodEndDate', label: 'окончание периода · короткая дата' },
  { value: 'PeriodStartMonthName', label: 'месяц начала периода · словом' },
  { value: 'PeriodEndMonthName', label: 'месяц окончания периода · словом' },
];

export function UtilityPanel(props: UtilityPanelProps) {
  function toggleFolderPart(part: FolderNamePartDto, checked: boolean) {
    const next = checked
      ? [...new Set([...props.folderParts, part])]
      : props.folderParts.filter((value) => value !== part);
    props.onFolderPartsChange(next);
  }

  return (
    <section className="settingsPanel" aria-label="Настройки программы">
      <div className="settingsSectionHeader">
        <div><strong>Основные настройки</strong><small>Результат, автоматическая обработка, обновления и лицензия.</small></div>
      </div>
      <div className="utilityGrid primarySettingsGrid">
        <div className="utilityCard outputSettingsCard">
          <strong>Папка готовых документов</strong>
          <div className="inlineInput folderPicker">
            <input
              value={props.outputRoot}
              onChange={(event) => props.onOutputRootChange(event.target.value)}
              placeholder="Например: C:\\Документы\\Готовые"
              aria-label="Папка готовых документов"
            />
            <button className="softBtn" type="button" onClick={props.onPickOutputFolder}><i className="ti ti-folder" aria-hidden="true" /> Выбрать</button>
          </div>
          <small className={props.outputRoot.trim() === props.savedOutputRoot.trim() && props.savedOutputRoot.trim() ? 'okText' : ''}>
            {props.outputRoot.trim() === props.savedOutputRoot.trim() && props.savedOutputRoot.trim()
              ? 'Путь проверен записью на диск и сохранён.'
              : props.outputRoot.trim()
                ? 'Изменение ещё не сохранено. Генерация продолжит использовать последний проверенный путь.'
                : props.savedOutputRoot.trim()
                  ? `Поле очищено, но сохранённый путь пока остаётся: ${props.savedOutputRoot}`
                  : 'Папка ещё не сохранена.'}
          </small>
          <button className="utilBtn" onClick={props.onSaveOutputFolder}>
            <i className="ti ti-device-floppy" aria-hidden="true" /> Проверить и сохранить папку
          </button>
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
          <small>Если не выбрать ни одного компонента, подпапка будет называться «Созданные документы». Персональные данные добавляются только явно.</small>
          <button className="utilBtn" onClick={props.onOutputPlan} disabled={!props.savedOutputRoot.trim()}>
            <i className="ti ti-folder" aria-hidden="true" /> Показать путь следующего комплекта
          </button>
        </div>

        <div className="utilityCard">
          <strong>Автоматическая обработка</strong>
          <small>Фоновый агент замечает новые файлы в рабочей папке и создаёт комплект без ручного запуска.</small>
          <button className="utilBtn" onClick={props.onInstallWatcher}>
            <i className="ti ti-eye-cog" aria-hidden="true" /> Включить фоновый агент
          </button>
          <button className="utilBtn" onClick={props.onUninstallWatcher}>
            <i className="ti ti-eye-off" aria-hidden="true" /> Отключить фоновый агент
          </button>
        </div>

        <div className="utilityCard">
          <strong>Программа и доступ</strong>
          <button className="utilBtn" onClick={props.onCheckUpdates}>
            <i className="ti ti-refresh" aria-hidden="true" /> Проверить обновления
          </button>
          <button className="utilBtn" onClick={props.onCheckAccess}>
            <i className="ti ti-shield-check" aria-hidden="true" /> Проверить доступ
          </button>
          <div className="licenseStack">
            <input
              value={props.licenseText}
              placeholder="Вставьте подписанную лицензию"
              onChange={(event) => props.onLicenseTextChange(event.target.value)}
            />
            <button className="softBtn" onClick={props.onVerifyLicense}>Активировать лицензию</button>
          </div>
        </div>
      </div>

      <details className="expertSettings">
        <summary>Экспертные и административные инструменты</summary>
        <p>Разметка, серии документов, сохранение сессий, реестры, обучение шаблонов и управление качеством. Для ежедневного создания документов этот раздел не требуется.</p>
        <div className="utilityGrid expertSettingsGrid">
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

          <button className="utilBtn" onClick={props.onSaveSession}>
            <i className="ti ti-database-export" aria-hidden="true" /> Сохранить сессию
          </button>
          <button className="utilBtn" onClick={props.onLoadSession}>
            <i className="ti ti-database-import" aria-hidden="true" /> Загрузить сессию
          </button>
          <BusinessRegistryPanel outputRoot={props.savedOutputRoot} onStatus={props.onStatus} onCaseChanged={(semanticCase) => props.onSemanticCaseChanged?.(semanticCase)} />
          <OrganizationKnowledgePanel onStatus={props.onStatus} onCaseChanged={(semanticCase) => props.onSemanticCaseChanged?.(semanticCase)} />
          <AdvancedToolsPanel documents={props.documents} selectedDocumentIds={props.selectedDocumentIds} outputRoot={props.savedOutputRoot} onStatus={props.onStatus} onDocumentsChanged={props.onDocumentsChanged} />
          <AutomationControlCenter onStatus={props.onStatus} />
          <LearningGovernancePanel documents={props.documents} onStatus={props.onStatus} />
        </div>
      </details>
    </section>
  );

}
