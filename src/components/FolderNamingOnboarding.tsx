import { useMemo, useState } from 'react';
import type { FolderNamePartDto } from '../lib/types';

const PARTS: Array<{ value: FolderNamePartDto; label: string; example: string }> = [
  { value: 'ShortInitials', label: 'Фамилия и инициалы', example: 'Иванов И.И.' },
  { value: 'SurnameGivenName', label: 'Фамилия и имя', example: 'Иванов Иван' },
  { value: 'FullSubjectName', label: 'Полное имя', example: 'Иванов Иван Иванович' },
  { value: 'OrganizationName', label: 'Организация / контрагент', example: 'ООО Ромашка' },
  { value: 'DocumentNumber', label: 'Номер документа / дела', example: '42' },
  { value: 'DocumentDate', label: 'Дата документа', example: '18.06.2026' },
  { value: 'PeriodRange', label: 'Период полностью', example: '01.06.2026 - 30.06.2026' },
  { value: 'ShortPeriodRange', label: 'Период коротко', example: '01.06.26-30.06.26' },
  { value: 'PeriodStartMonthName', label: 'Месяц начала словом', example: 'июнь 2026' },
  { value: 'PeriodEndMonthName', label: 'Месяц окончания словом', example: 'июнь 2026' },
];

const PRESETS: Array<{ id: string; title: string; hint: string; parts: FolderNamePartDto[] }> = [
  { id: 'person-month', title: 'Человек + месяц', hint: 'Иванов И.И. июнь 2026', parts: ['ShortInitials', 'PeriodStartMonthName'] },
  { id: 'organization-number', title: 'Организация + номер', hint: 'ООО Ромашка 42', parts: ['OrganizationName', 'DocumentNumber'] },
  { id: 'person-number', title: 'Человек + номер', hint: 'Петров П.П. 18', parts: ['ShortInitials', 'DocumentNumber'] },
  { id: 'number-date', title: 'Номер + дата', hint: '127 18.06.2026', parts: ['DocumentNumber', 'DocumentDate'] },
  { id: 'organization-period', title: 'Организация + период', hint: 'ООО Ромашка 01.06.26-30.06.26', parts: ['OrganizationName', 'ShortPeriodRange'] },
];

export function FolderNamingOnboarding(props: {
  currentRoot: string;
  currentParts: FolderNamePartDto[];
  onPickRoot(): void;
  onConfirm(parts: FolderNamePartDto[]): void;
}) {
  const [selected, setSelected] = useState<FolderNamePartDto[]>(props.currentParts);
  const [advanced, setAdvanced] = useState(false);
  const preview = useMemo(() => {
    const byId = new Map(PARTS.map(part => [part.value, part.example]));
    const chunks = selected.map(part => byId.get(part)).filter((value): value is string => Boolean(value));
    return chunks.join(' ') || 'Выберите хотя бы один компонент имени';
  }, [selected]);
  const root = props.currentRoot.trim();

  function toggle(part: FolderNamePartDto) {
    setSelected(current => current.includes(part) ? current.filter(value => value !== part) : [...current, part]);
  }

  return (
    <div className="backdrop folderNamingBackdrop" role="presentation">
      <section className="modal folderNamingOnboarding" role="dialog" aria-modal="true" aria-labelledby="folder-naming-title">
        <span className="workflowEyebrow">Первичная настройка результата</span>
        <h2 id="folder-naming-title">Как называть папку комплекта?</h2>
        <p className="hint">Сначала выберите реальную папку на компьютере, куда программа будет складывать готовые документы. Затем задайте правило имени подпапки. Оба значения сохраняются.</p>

        <div className="folderNamingPreview" data-testid="output-root-choice">
          <span>Куда сохранять готовые документы</span>
          <strong title={root}>{root || 'Папка ещё не выбрана'}</strong>
          <button type="button" className="softBtn" onClick={props.onPickRoot}>Выбрать папку на компьютере</button>
          <small>После создания программа отдельно покажет точный путь и список созданных файлов.</small>
        </div>

        <div className="folderNamingPresets" role="group" aria-label="Готовые правила имени папки">
          {PRESETS.map(preset => {
            const active = preset.parts.length === selected.length && preset.parts.every((part, index) => selected[index] === part);
            return (
              <button key={preset.id} type="button" className={`folderNamingPreset ${active ? 'selected' : ''}`} onClick={() => setSelected(preset.parts)}>
                <strong>{preset.title}</strong>
                <small>{preset.hint}</small>
              </button>
            );
          })}
        </div>

        <button type="button" className="textBtn folderNamingAdvancedToggle" onClick={() => setAdvanced(value => !value)}>
          {advanced ? 'Скрыть точную настройку' : 'Настроить состав имени вручную'}
        </button>
        {advanced && (
          <fieldset className="folderNamingParts">
            <legend>Что включать в имя папки</legend>
            {PARTS.map(part => (
              <label key={part.value}>
                <input type="checkbox" checked={selected.includes(part.value)} onChange={() => toggle(part.value)} />
                <span>{part.label}</span>
              </label>
            ))}
          </fieldset>
        )}

        <div className="folderNamingPreview">
          <span>Пример</span>
          <strong>{preview}</strong>
          {!selected.length && <small>Пустое имя запрещено: одинаковая подпапка для разных комплектов небезопасна.</small>}
        </div>

        <div className="modalActions">
          <small>{!root ? 'Сначала выберите папку на компьютере.' : !selected.length ? 'Выберите хотя бы один компонент имени подпапки.' : 'Папка и правило будут сохранены.'}</small>
          <span className="spacer" />
          <button type="button" className="primaryBtn" autoFocus aria-keyshortcuts="Enter" disabled={!root || !selected.length} onClick={() => props.onConfirm(selected)}>Сохранить папку и правило</button>
        </div>
      </section>
    </div>
  );
}
