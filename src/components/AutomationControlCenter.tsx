import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { AuditEventRecord, AutomationExceptionRecord, AutomationMetrics, DailyAutomationDashboard, CaseRunRecord, LocalSemanticModelConfig, LocalSemanticModelStatus, CorpusStatus, CalibratedThresholdStatus, ReferenceDataStatus, QueueStatus, PrinterInventory, PrivacyPreferences, SidecarToolStatus, ComponentProgress, ComponentStatus, QualityTelemetryReport } from '../lib/types';
import { confirmRiskExceptionAndRetry, confirmBundleExceptionAndRetry, getAutomationMetrics, getDailyAutomationDashboard, getQualityTelemetry, getQueueStatus, getCorpusStatus, exportCorpus, getCalibratedThresholdStatus, importCalibratedThresholdsFile, getPrinterInventory, listCaseRuns, retryCaseRun, getPrivacyPreferences, getSemanticModelConfig, getReferenceDataStatus, getSidecarStatus, getComponentStatuses, installComponent, refreshComponentCatalog, removeComponent, listAuditEvents, listAutomationExceptions, resolveAutomationException, runWorkspaceHygiene, testSemanticModel, updatePrintPreferences, updatePrivacyPreferences, updateReferenceData, importReferenceDataFile, updateSemanticModelConfig } from '../lib/api';

interface Props { onStatus(message: string): void; }

const DEFAULT_PRIVACY: PrivacyPreferences = {
  copy_source_to_output: false,
  write_trust_report: true,
  include_values_in_trust_report: false,
  temp_retention_hours: 4,
  archive_processed_sources: true,
  archive_folder_name: '_обработано',
  service_note_retention_days: 30,
  processed_marker_retention_days: 7,
  archived_source_retention_days: 365,
};

const DEFAULT_MODEL: LocalSemanticModelConfig = {
  enabled: false,
  provider: 'ollama',
  endpoint: 'http://127.0.0.1:11434',
  model: 'qwen2.5:7b-instruct',
  preferred_language: 'auto',
  timeout_seconds: 90,
  shadow_mode: true,
  corpus_recording_enabled: false,
  auto_apply_zero_touch: false,
  consistency_passes: 2,
};

const DEFAULT_PRINTERS: PrinterInventory = {
  platform: '',
  printers: [],
  preferences: { printer_name: null, duplex_mode: 'simplex', tray: null },
  advanced_options_note: '',
};

export function AutomationControlCenter({ onStatus }: Props) {
  const [privacy, setPrivacy] = useState<PrivacyPreferences>(DEFAULT_PRIVACY);
  const [exceptions, setExceptions] = useState<AutomationExceptionRecord[]>([]);
  const [caseRuns, setCaseRuns] = useState<CaseRunRecord[]>([]);
  const [metrics, setMetrics] = useState<AutomationMetrics | null>(null);
  const [daily, setDaily] = useState<DailyAutomationDashboard | null>(null);
  const [qualityTelemetry, setQualityTelemetry] = useState<QualityTelemetryReport | null>(null);
  const [queueStatus, setQueueStatus] = useState<QueueStatus | null>(null);
  const [corpusStatus, setCorpusStatus] = useState<CorpusStatus | null>(null);
  const [calibratedThresholds, setCalibratedThresholds] = useState<CalibratedThresholdStatus[]>([]);
  const [audit, setAudit] = useState<AuditEventRecord[]>([]);
  const [model, setModel] = useState<LocalSemanticModelConfig>(DEFAULT_MODEL);
  const [modelStatus, setModelStatus] = useState<LocalSemanticModelStatus | null>(null);
  const [referenceData, setReferenceData] = useState<ReferenceDataStatus | null>(null);
  const [sidecars, setSidecars] = useState<SidecarToolStatus[]>([]);
  const [components, setComponents] = useState<ComponentStatus[]>([]);
  const [componentProgress, setComponentProgress] = useState<ComponentProgress | null>(null);
  const [printers, setPrinters] = useState<PrinterInventory>(DEFAULT_PRINTERS);
  const [includeResolved, setIncludeResolved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [minutesPerDocument, setMinutesPerDocument] = useState(() => { const stored = Number(globalThis.localStorage?.getItem('dokkomplekt.roi.minutesPerDocument') ?? 7); return Number.isFinite(stored) && stored >= 0 ? stored : 7; });

  useEffect(() => { void refresh(false); }, []);
  useEffect(() => {
    let stop: (() => void) | undefined;
    void listen<ComponentProgress>('component://progress', event => setComponentProgress(event.payload))
      .then(unlisten => { stop = unlisten; })
      .catch(() => undefined);
    return () => stop?.();
  }, []);

  async function execute<T>(label: string, action: () => Promise<T>): Promise<T | undefined> {
    setBusy(true);
    try { return await action(); }
    catch (error) { onStatus(`Ошибка «${label}»: ${message(error)}`); return undefined; }
    finally { setBusy(false); }
  }

  async function refresh(showResolved = includeResolved) {
    const result = await execute('центр автоматизации', async () => Promise.all([
      getPrivacyPreferences(),
      listAutomationExceptions(showResolved),
      getAutomationMetrics(),
      getDailyAutomationDashboard(),
      getQueueStatus(),
      getCorpusStatus(),
      getCalibratedThresholdStatus(),
      listCaseRuns(50),
      listAuditEvents(20),
      getSemanticModelConfig(),
      getReferenceDataStatus(),
      getComponentStatuses(),
      getSidecarStatus(),
      getPrinterInventory(),
      getQualityTelemetry(),
    ]));
    if (!result) return;
    setPrivacy(result[0]); setExceptions(result[1]); setMetrics(result[2]); setDaily(result[3]); setQueueStatus(result[4]); setCorpusStatus(result[5]); setCalibratedThresholds(result[6]); setCaseRuns(result[7]); setAudit(result[8]);
    setModel(result[9].config); setModelStatus(result[9].status);
    setReferenceData(result[10]);
    setComponents(result[11]);
    setSidecars(result[12]);
    setPrinters(result[13]);
    setQualityTelemetry(result[14]);
  }

  async function savePrivacy() {
    const normalized = {
      ...privacy,
      archive_folder_name: privacy.archive_folder_name.trim() || '_обработано',
      temp_retention_hours: Math.max(1, Math.min(720, Math.trunc(privacy.temp_retention_hours || 24))),
      service_note_retention_days: Math.max(1, Math.min(3650, Math.trunc(privacy.service_note_retention_days || 30))),
      processed_marker_retention_days: Math.max(1, Math.min(3650, Math.trunc(privacy.processed_marker_retention_days || 7))),
      archived_source_retention_days: Math.max(0, Math.min(3650, Math.trunc(privacy.archived_source_retention_days || 0))),
    };
    const saved = await execute('политика конфиденциальности', () => updatePrivacyPreferences(normalized));
    if (!saved) return;
    setPrivacy(saved); onStatus('Политика конфиденциальности сохранена и записана в журнал аудита.');
  }

  async function cleanWorkspaceNow() {
    const report = await execute('гигиена рабочей папки', () => runWorkspaceHygiene());
    if (!report) return;
    const changed = report.archived_processed_sources.length + report.archived_service_files.length + report.removed_orphan_markers.length + report.removed_expired_archived_files.length;
    onStatus(changed > 0
      ? `Рабочая папка очищена: обработано служебных объектов — ${changed}.`
      : report.warnings.length > 0
        ? `Очистка завершена с предупреждениями: ${report.warnings[0]}`
        : 'Рабочая папка уже чистая.');
    await refresh();
  }

  async function resolve(item: AutomationExceptionRecord) {
    const resolution = globalThis.prompt?.('Что исправлено или подтверждено?', 'Проверено специалистом')?.trim();
    if (!resolution) return;
    const ok = await execute('закрытие исключения', () => resolveAutomationException(item.exception_id, resolution));
    if (ok) { onStatus('Исключение закрыто. Повторите обработку источника после исправления причины.'); await refresh(); }
  }

  async function confirmAllRiskValues(item: AutomationExceptionRecord) {
    const details = parseExceptionDetails(item.details_json);
    const blockers = Array.isArray(details?.blockers) ? details.blockers : [];
    const fieldIds = blockers.map((entry: unknown) => typeof entry === 'object' && entry !== null && 'field_id' in entry ? String((entry as { field_id: unknown }).field_id) : '').filter(Boolean);
    const accepted = globalThis.confirm?.(`Подтвердить найденные значения сразу для ${fieldIds.length} полей и повторить создание комплекта?\n\n${fieldIds.join('\n')}`) ?? false;
    if (!accepted) return;
    const result = await execute('подтверждение спорных данных', () => confirmRiskExceptionAndRetry(item.exception_id));
    if (!result) return;
    onStatus(result.message);
    await refresh();
  }

  async function confirmBundle(item: AutomationExceptionRecord) {
    const details = parseExceptionDetails(item.details_json);
    const proposed = Array.isArray(details?.proposed_document_ids)
      ? details.proposed_document_ids.map((value: unknown) => String(value)).filter(Boolean)
      : [];
    if (!proposed.length) {
      onStatus('В исключении нет безопасного предложения состава. Откройте исходный случай и выберите документы вручную.');
      return;
    }
    const accepted = globalThis.confirm?.(`Создать только этот комплект?\n\n${proposed.join('\n')}`) ?? false;
    if (!accepted) return;
    const result = await execute('подтверждение состава комплекта', () => confirmBundleExceptionAndRetry(item.exception_id, proposed));
    if (!result) return;
    onStatus(result.message);
    await refresh();
  }

  async function retryCase(item: CaseRunRecord) {
    const result = await execute(item.status === 'completed' ? 'переиздание дела' : 'повтор дела', () => retryCaseRun(item.case_id));
    if (!result) return;
    onStatus(result.message);
    await refresh();
  }

  async function saveModel() {
    const normalized: LocalSemanticModelConfig = {
      ...model,
      endpoint: model.endpoint.trim(),
      model: model.model.trim(),
      timeout_seconds: Math.max(5, Math.min(600, Math.trunc(model.timeout_seconds || 90))),
      consistency_passes: Math.max(2, Math.min(3, Math.trunc(model.consistency_passes || 2))),
    };
    const saved = await execute('настройки локального анализа', () => updateSemanticModelConfig(normalized));
    if (!saved) return;
    setModel(saved.config); setModelStatus(saved.status);
    onStatus('Настройки локального анализа сохранены. Документы обрабатываются только на этом компьютере и не отправляются во внешнюю сеть.');
  }

  async function checkModel() {
    const status = await execute('проверка локального анализа', () => testSemanticModel());
    if (!status) return;
    setModelStatus(status);
    onStatus(status.message);
  }

  async function exportPilotCorpus() {
    const date = new Date().toISOString().slice(0, 10);
    const outputPath = globalThis.prompt?.('Имя файла с обезличенной историей проверок', `dokkomplekt-check-history-${date}.json`)?.trim();
    if (!outputPath) return;
    const result = await execute('экспорт истории проверок', () => exportCorpus(outputPath));
    if (!result) return;
    onStatus(`Экспортировано записей проверки: ${result.entry_count}. Файл: ${result.output_path}`);
    await refresh();
  }

  async function reloadComponentCatalog() {
    const result = await execute('каталог компонентов', refreshComponentCatalog);
    if (result) { setComponents(result); setSidecars(await getSidecarStatus()); onStatus('Подписанный каталог компонентов обновлён.'); }
  }

  async function downloadComponent(item: ComponentStatus) {
    const result = await execute(`компонент ${item.label}`, () => installComponent(item.id));
    if (result) {
      setComponents(current => current.map(component => component.id === result.id ? result : component));
      setSidecars(await getSidecarStatus());
      onStatus(`${result.label}: компонент установлен и доступен офлайн.`);
    }
  }

  async function deleteComponent(item: ComponentStatus) {
    if (!globalThis.confirm?.(`Удалить компонент «${item.label}»? Функции снова станут недоступны офлайн.`)) return;
    const result = await execute(`удаление ${item.label}`, () => removeComponent(item.id));
    if (result) {
      setComponents(current => current.map(component => component.id === result.id ? result : component));
      setSidecars(await getSidecarStatus());
      onStatus(`${result.label}: компонент удалён.`);
    }
  }

  async function importSignedThresholds(file: File) {
    if (!/\.json$/i.test(file.name)) {
      onStatus('Подписанный пакет порогов должен быть JSON-файлом.');
      return;
    }
    if (file.size <= 0 || file.size > 1024 * 1024) {
      onStatus('Размер подписанного пакета порогов должен быть от 1 байта до 1 МБ.');
      return;
    }
    const bytes = await readFileBytes(file);
    let binary = '';
    const chunk = 0x8000;
    for (let offset = 0; offset < bytes.length; offset += chunk) {
      binary += String.fromCharCode(...bytes.subarray(offset, offset + chunk));
    }
    const status = await execute('импорт подписанных порогов автопечати', () => importCalibratedThresholdsFile(file.name, btoa(binary)));
    if (!status) return;
    setCalibratedThresholds(current => [...current.filter(item => item.domain !== status.domain), status].sort((a, b) => a.domain.localeCompare(b.domain)));
    onStatus(`Настройки автопечати для набора «${status.domain}» проверены. Используются только подписанные контрольные показатели.`);
  }

  async function refreshReferenceData() {
    const status = await execute('обновление производственного календаря', () => updateReferenceData());
    if (!status) return;
    setReferenceData(status);
    onStatus(status.message);
  }

  async function importSignedReferenceData(file: File) {
    if (!/\.json$/i.test(file.name)) {
      onStatus('Подписанный календарный пакет должен быть JSON-файлом.');
      return;
    }
    if (file.size <= 0 || file.size > 4 * 1024 * 1024) {
      onStatus('Размер подписанного календарного пакета должен быть от 1 байта до 4 МБ.');
      return;
    }
    const bytes = await readFileBytes(file);
    let binary = '';
    const chunk = 0x8000;
    for (let offset = 0; offset < bytes.length; offset += chunk) {
      binary += String.fromCharCode(...bytes.subarray(offset, offset + chunk));
    }
    const status = await execute('импорт подписанного производственного календаря', () => importReferenceDataFile(file.name, btoa(binary)));
    if (!status) return;
    setReferenceData(status);
    onStatus(status.message);
  }

  async function savePrinterPreferences() {
    const saved = await execute('настройки печати', () => updatePrintPreferences(printers.preferences));
    if (!saved) return;
    setPrinters(saved);
    onStatus('Настройки принтера сохранены и будут использоваться ручной печатью и фоновым агентом.');
  }

  async function toggleResolved(value: boolean) { setIncludeResolved(value); await refresh(value); }

  return <div className="automationControlCenter">
    <section className="utilityCard advancedCard semanticModelCard">
      <strong>Локальное понимание документов</strong>
      <small>Подключите локальный модуль анализа текста. Программа перепроверит найденные значения, форматы и контрольные суммы перед применением.</small>
      <label><input type="checkbox" checked={model.enabled} onChange={e => setModel({ ...model, enabled: e.target.checked })}/> включить локальное понимание свободного текста</label>
      <label>Способ подключения<select value={model.provider} onChange={e => setModel({ ...model, provider: e.target.value })}><option value="ollama">Ollama</option><option value="llama_cpp">llama.cpp / OpenAI-compatible</option></select></label>
      <label>Локальный адрес<input value={model.endpoint} onChange={e => setModel({ ...model, endpoint: e.target.value })} placeholder="http://127.0.0.1:11434" /></label>
      <label>Модель<input value={model.model} onChange={e => setModel({ ...model, model: e.target.value })} placeholder="qwen2.5:7b-instruct" /></label>
      <label>Язык документов<select value={model.preferred_language} onChange={e => setModel({ ...model, preferred_language: e.target.value })}><option value="auto">Автоопределение</option><option value="ru-RU">Русский</option><option value="en-US">English</option><option value="de-DE">Deutsch</option><option value="fr-FR">Français</option><option value="es-ES">Español</option><option value="uk-UA">Українська</option><option value="kk-KZ">Қазақша</option><option value="zh-CN">中文</option><option value="ar">العربية</option></select></label>
      <label>Тайм-аут, секунд<input type="number" min={5} max={600} value={model.timeout_seconds} onChange={e => setModel({ ...model, timeout_seconds: Number(e.target.value) })}/></label>
      <label>Независимых проходов<select value={model.consistency_passes} onChange={e => setModel({ ...model, consistency_passes: Number(e.target.value) })}><option value={2}>2 — обязательная взаимная проверка</option><option value={3}>3 — максимум проверки</option></select></label>
      <small>Для важных полей значение принимается только тогда, когда минимум две независимые проверки дали одинаковый результат.</small>
      <label><input type="checkbox" checked={model.shadow_mode} onChange={e => setModel({ ...model, shadow_mode: e.target.checked, auto_apply_zero_touch: e.target.checked ? false : model.auto_apply_zero_touch })}/> Режим наблюдения: сравнивать результаты, но не изменять документы</label>
      <label><input type="checkbox" checked={model.corpus_recording_enabled} onChange={e => setModel({ ...model, corpus_recording_enabled: e.target.checked })}/> с согласия пользователя сохранять обезличенную историю проверок и исправлений</label>
      <label><input type="checkbox" disabled={model.shadow_mode} checked={model.auto_apply_zero_touch} onChange={e => setModel({ ...model, auto_apply_zero_touch: e.target.checked })}/> использовать локальный анализ при автоматической обработке; сомнительные значения всё равно потребуют проверки</label>
      <div className="inlineButtons"><button className="utilBtn" disabled={busy} onClick={saveModel}>Сохранить</button><button className="softBtn" disabled={busy || !model.enabled} onClick={checkModel}>Проверить соединение</button><button className="softBtn" disabled={busy || !corpusStatus || corpusStatus.entry_count === 0} onClick={() => void exportPilotCorpus()}>Экспортировать историю проверок</button></div>
      {corpusStatus && <div className="modelStatus ok"><b>История проверок: {corpusStatus.entry_count} записей</b><span>{corpusStatus.message}</span><small>Сырые тексты и значения не экспортируются; локальное хранилище зашифровано.</small></div>}
      {modelStatus && <div className={modelStatus.reachable ? 'modelStatus ok' : 'modelStatus'}><b>{modelStatus.reachable ? 'Доступна' : modelStatus.configured ? 'Не подключена' : 'Некорректная настройка'}</b><span>{modelStatus.message}</span>{modelStatus.available_models.length > 0 && <small>Доступные модели: {modelStatus.available_models.join(', ')}</small>}</div>}
    </section>


    <section className="utilityCard advancedCard">
      <strong>Безопасная автопечать</strong>
      <small>Автопечать включается только после подтверждённой проверки качества. Пока такой проверки нет, комплект создаётся для просмотра перед печатью.</small>
      <div className="compactList sidecarList">
        {calibratedThresholds.length === 0 && <small>Подтверждённые настройки качества не установлены — автоматическая печать пока недоступна.</small>}
        {calibratedThresholds.map(item => <div key={item.domain} className="sidecarReady">
          <span><b>{item.domain}</b> · порог автопечати ≥ {(item.auto_min_confidence * 100).toFixed(1)}%<small>{item.message}</small><small>Проверено примеров: {item.training_observations}; контрольных примеров: {item.holdout_observations}; допустимая ошибка: {(item.max_auto_error_rate * 100).toFixed(2)}%</small></span>
        </div>)}
      </div>
      <label className="softBtn fileButton" aria-label="Импортировать подписанные пороги автопечати">
        Импортировать пороги
        <input type="file" accept="application/json,.json" disabled={busy} onChange={event => { const file = event.currentTarget.files?.[0]; if (file) void importSignedThresholds(file); event.currentTarget.value = ''; }} />
      </label>
    </section>

    <section className="utilityCard advancedCard">
      <strong>Производственный календарь</strong>
      <small>Календарь обновляется только проверенным подписанным пакетом. Неподтверждённые годы не используются в расчётах.</small>
      {referenceData ? <><div className="modelStatus ok"><b>{referenceData.installed ? 'Подписанный календарь активен' : referenceData.cached ? 'Ожидает перезапуска' : 'Встроенный календарь'}</b><span>{referenceData.message}</span><small>Полные годы: {referenceData.complete_years.join(', ') || 'только встроенные данные'}</small></div><small className={calendarHorizon(referenceData.complete_years).urgent?'calendarWarning urgent':'calendarWarning'}>{calendarHorizon(referenceData.complete_years).message}</small></> : <small>Статус не загружен.</small>}
      <div className="inlineButtons">
        <button className="softBtn" disabled={busy} onClick={() => void refreshReferenceData()}>Проверить подписанное обновление</button>
        <label className="softBtn fileButton" aria-label="Импортировать подписанный календарь">
          Импортировать пакет
          <input type="file" accept="application/json,.json" disabled={busy} onChange={event => { const file = event.currentTarget.files?.[0]; if (file) void importSignedReferenceData(file); event.currentTarget.value = ''; }} />
        </label>
      </div>
    </section>

    <section className="utilityCard advancedCard">
      <strong>Дополнительные возможности</strong>
      <small>Дополнительные компоненты проверяются встроенной цифровой подписью и контрольной суммой. После установки они работают без интернета.</small>
      <div className="inlineButtons"><button className="softBtn" disabled={busy} onClick={() => void reloadComponentCatalog()}>Проверить подписанный каталог</button></div>
      {componentProgress && componentProgress.phase !== 'complete' && <div className="componentProgress"><progress max={100} value={componentProgress.percent}/><small>{componentProgress.message} · {componentProgress.percent}%</small></div>}
      <div className="compactList sidecarList">
        {components.map(item => <div key={item.id} className={item.available ? 'sidecarReady' : 'sidecarMissing'}>
          <span><b>{item.label}</b> · {item.size_label}<small>{item.message}</small><small>Статус: {componentStateLabel(item.state)} · добавляет: {item.unlocks.join(', ')}</small></span>
          <div className="inlineButtons">{item.state === 'downloaded' ? <button className="softBtn" disabled={busy} onClick={() => void deleteComponent(item)}>Удалить</button> : item.state === 'missing' ? <button className="utilBtn" disabled={busy} onClick={() => void downloadComponent(item)}>Скачать</button> : <small>Доступен</small>}</div>
        </div>)}
      </div>
      <details><summary>Технический статус инструментов</summary><div className="compactList sidecarList">
        {sidecars.map(item => <div key={item.tool} className={item.available ? 'sidecarReady' : 'sidecarMissing'}>
          <span><b>{item.tool}</b> · {item.purpose}<small>{componentStateLabel(item.state)} · {item.resolved_path}</small></span>
        </div>)}
      </div></details>
    </section>

    <section className="utilityCard advancedCard semanticModelCard">
      <strong>Принтер и параметры вывода</strong>
      <label>Принтер<select value={printers.preferences.printer_name ?? ''} onChange={e => setPrinters({ ...printers, preferences: { ...printers.preferences, printer_name: e.target.value || null } })}><option value="">системный по умолчанию</option>{printers.printers.map(item => <option key={item.name} value={item.name}>{item.name}{item.is_default ? ' · по умолчанию' : ''}</option>)}</select></label>
      <label>Двусторонняя печать<select value={printers.preferences.duplex_mode} onChange={e => setPrinters({ ...printers, preferences: { ...printers.preferences, duplex_mode: e.target.value } })}><option value="simplex">односторонняя</option><option value="long_edge">по длинной стороне</option><option value="short_edge">по короткой стороне</option><option value="manual">ручная двусторонняя печать</option></select></label>
      <label>Лоток Word<select value={printers.preferences.tray ?? ''} onChange={e => setPrinters({ ...printers, preferences: { ...printers.preferences, tray: e.target.value === '' ? null : Number(e.target.value) } })}><option value="">по умолчанию</option><option value="0">драйвер по умолчанию</option><option value="1">верхний</option><option value="2">нижний</option><option value="3">средний</option><option value="4">ручная подача</option><option value="7">автоматическая подача</option><option value="10">крупный формат</option><option value="11">большая ёмкость</option><option value="14">кассета</option><option value="15">источник формы</option></select></label>
      <button className="utilBtn" disabled={busy} onClick={savePrinterPreferences}>Сохранить печать</button>
      <small>{printers.advanced_options_note || 'Список принтеров пока не загружен.'}</small>
    </section>

    <section className="utilityCard advancedCard">
      <strong>Дела и восстановление после сбоя</strong>
      <small>Каждая автоматическая обработка имеет понятный статус. Незавершённую задачу можно повторить, а готовый комплект — создать заново без удаления прежнего результата.</small>
      <div className="compactList caseRunList">
        {caseRuns.length === 0 && <small>Обработанных дел пока нет.</small>}
        {caseRuns.map(item => <div key={item.case_id} className={`caseRunItem case-${item.status}`}>
          <span><b>{safeSource(item.source_path)}</b> · {caseStatusLabel(item.status)}<small>{item.last_error || item.patient_folder || `SHA-256 ${item.source_sha256.slice(0, 12)}…`}</small></span>
          {!['normalizing', 'recognizing', 'checking', 'generating', 'publishing'].includes(item.status) && <button className="softBtn" disabled={busy} onClick={() => retryCase(item)}>{item.status === 'completed' ? 'Переиздать' : 'Повторить'}</button>}
        </div>)}
      </div>
    </section>

    <section className="utilityCard advancedCard">
      <strong>Конфиденциальность и хранение</strong>
      <label><input type="checkbox" checked={privacy.copy_source_to_output} onChange={e => setPrivacy({ ...privacy, copy_source_to_output: e.target.checked })}/> копировать первичный источник в готовый комплект</label>
      <label><input type="checkbox" checked={privacy.write_trust_report} onChange={e => setPrivacy({ ...privacy, write_trust_report: e.target.checked })}/> создавать локальный отчёт проверяемости</label>
      <label><input type="checkbox" checked={privacy.include_values_in_trust_report} onChange={e => setPrivacy({ ...privacy, include_values_in_trust_report: e.target.checked })}/> включать значения полей в отчёт</label>
      <label>Хранить временные источники, часов<input type="number" min={1} max={720} value={privacy.temp_retention_hours} onChange={e => setPrivacy({ ...privacy, temp_retention_hours: Number(e.target.value) })}/></label>
      <label><input type="checkbox" checked={privacy.archive_processed_sources} onChange={e => setPrivacy({ ...privacy, archive_processed_sources: e.target.checked })}/> перемещать успешно обработанные источники из рабочей папки в архив</label>
      <label>Подпапка архива<input value={privacy.archive_folder_name} onChange={e => setPrivacy({ ...privacy, archive_folder_name: e.target.value })} placeholder="_обработано" /></label>
      <label>Архивировать служебные заметки через, дней<input type="number" min={1} max={3650} value={privacy.service_note_retention_days} onChange={e => setPrivacy({ ...privacy, service_note_retention_days: Number(e.target.value) })}/></label>
      <label>Удалять устаревшие служебные отметки через, дней<input type="number" min={1} max={3650} value={privacy.processed_marker_retention_days} onChange={e => setPrivacy({ ...privacy, processed_marker_retention_days: Number(e.target.value) })}/></label>
      <label>Удалять архивные источники через, дней<input type="number" min={0} max={3650} value={privacy.archived_source_retention_days} onChange={e => setPrivacy({ ...privacy, archived_source_retention_days: Number(e.target.value) })}/><small>0 — хранить бессрочно.</small></label>
      <div className="inlineButtons"><button className="utilBtn" disabled={busy} onClick={savePrivacy}>Сохранить политику</button><button className="softBtn" disabled={busy} onClick={cleanWorkspaceNow}>Очистить сейчас</button></div>
    </section>

    <section className="utilityCard advancedCard">
      <strong>Межкомпьютерная очередь</strong>
      <small>{queueStatus?.message ?? 'Статус очереди ещё не загружен.'}</small>
      <div className="metricGrid">
        <span>Режим <b>{queueStatus?.mode === 'central_mtls' ? 'защищённая центральная' : queueStatus?.mode === 'configuration_error' ? 'ошибка настройки' : 'локальная файловая'}</b></span>
        <span>Доступность <b>{queueStatus?.reachable ? 'готова' : 'нет соединения'}</b></span>
      </div>
      {queueStatus?.mode === 'shared_filesystem' && <small>Локальная файловая очередь работает без интернета и подходит для одного ПК или общей папки малого офиса. Для нескольких компьютеров используйте защищённую центральную очередь с отдельным доступом для каждого устройства.</small>}
    </section>

    <section className="utilityCard advancedCard">
      <strong>Очередь исключений</strong>
      <label><input type="checkbox" checked={includeResolved} onChange={e => void toggleResolved(e.target.checked)}/> показывать закрытые</label>
      <button className="softBtn" disabled={busy} onClick={() => void refresh()}>Обновить</button>
      <div className="compactList">
        {exceptions.length === 0 && <small>Неразрешённых остановок нет.</small>}
        {exceptions.map(item => <div key={item.exception_id} className="exceptionItem">
          <span><b>{exceptionCategoryLabel(item.category)}</b> · {item.message}<small>{safeSource(item.source_path)} · {item.created_at}</small></span>
          {item.status !== 'resolved' && <div className="inlineButtons">{item.category === 'risk_gate' && <button className="utilBtn" disabled={busy} onClick={() => void confirmAllRiskValues(item)}>Подтвердить всё и продолжить</button>}{item.category === 'bundle_decision' && <button className="utilBtn" disabled={busy} onClick={() => void confirmBundle(item)}>Подтвердить предложенный комплект</button>}<button className="softBtn" disabled={busy} onClick={() => void resolve(item)}>Закрыть</button></div>}
        </div>)}
      </div>
    </section>

    <section className="utilityCard advancedCard ownerDashboardCard">
      <strong>Сегодня — только результат и исключения</strong>
      <small>Данные считаются локально по завершённым задачам и событиям печати за {daily?.date_utc ?? 'сегодня'}.</small>
      {daily ? <div className="ownerDashboardGrid">
        <span>Обработано дел<b>{daily.processed_cases}</b></span>
        <span>Автоматически завершено<b>{daily.automatically_completed_cases}</b></span>
        <span>Требуют внимания<b>{daily.attention_cases}</b></span>
        <span>Создано документов<b>{daily.generated_documents}</b></span>
        <span>Отправлено на печать<b>{daily.printed_documents}</b></span>
        <span>Сэкономлено ориентировочно<b>{formatMinutes(Math.max(0, daily.generated_documents * minutesPerDocument - daily.measured_processing_milliseconds / 60000))}</b></span>
      </div> : <small>Сводка за сегодня ещё не загружена.</small>}
    </section>

    <section className="utilityCard advancedCard">
      <strong>Результаты автоматизации</strong>
      <label>Минут ручной работы на один документ<input aria-label="Минут на документ" type="number" min={0} max={480} value={minutesPerDocument} onChange={event=>{const value=Math.max(0,Math.min(480,Number(event.target.value)||0));setMinutesPerDocument(value);globalThis.localStorage?.setItem('dokkomplekt.roi.minutesPerDocument',String(value))}}/><small>Оценка задаётся организацией и становится точной после замера на реальной работе.</small></label>
      {metrics ? <div className="metricGrid">
        <span>Источников <b>{metrics.processed_sources}</b></span><span>Документов <b>{metrics.generated_documents}</b></span>
        <span>Автоматически обработано <b>{metrics.zero_touch_sources}</b></span><span>Доля автоматической обработки <b>{percent(metrics.zero_touch_sources, metrics.processed_sources)}</b></span>
        <span>Остановлено <b>{metrics.blocked_sources}</b></span><span>Проверено вручную <b>{metrics.attention_resolutions}</b></span>
        <span>Ошибок <b>{metrics.failed_sources}</b></span><span>Сбоев печати <b>{metrics.print_failures}</b></span>
        <span>Подтверждений полей <b>{metrics.user_confirmations}</b></span><span>Отклонено сомнительных предложений <b>{metrics.model_grounding_rejections}</b></span>
        <span>Проверок в режиме наблюдения <b>{metrics.shadow_model_runs}</b></span><span>Совпадений результатов <b>{percent(metrics.shadow_model_agreements, metrics.shadow_model_proposals)}</b></span>
        <span>Документов на источник <b>{ratio(metrics.generated_documents, metrics.processed_sources)}</b></span><span>Доля требующих проверки <b>{percent(metrics.blocked_sources, metrics.processed_sources)}</b></span>
        <span>Использовано повторно <b>{metrics.reused_documents ?? 0}</b></span><span>Создано заново <b>{metrics.rerendered_documents ?? 0}</b></span>
        <span>Время автоматической обработки <b>{formatMinutes((metrics.processing_milliseconds ?? 0)/60000)}</b></span><span>На проверку перед печатью <b>{metrics.print_review_queued ?? 0}</b></span>
        <span>Прошло автопечать-gate <b>{metrics.automatic_print_approved ?? 0}</b></span><span>Успех без ошибок <b>{percent(Math.max(0,metrics.processed_sources-metrics.failed_sources), metrics.processed_sources)}</b></span>
        <span>Оценка сэкономленного времени <b>{formatMinutes(Math.max(0, metrics.generated_documents*minutesPerDocument-(metrics.processing_milliseconds ?? 0)/60000))}</b></span><span>Метод <b>базовая норма − замер runtime</b></span>
      </div> : <small>Метрики ещё не загружены.</small>}
    </section>

    <section className="utilityCard advancedCard qualityTelemetryCard">
      <strong>Локальная телеметрия качества</strong>
      <small>Считаются только локальные агрегаты без текста документов. Повторяющееся действие лишь предлагается как правило и никогда не включается скрытно.</small>
      {qualityTelemetry ? <>
        <div className="telemetryColumns">
          <TelemetryList title="Где останавливается автоматика" items={qualityTelemetry.stop_reasons} />
          <TelemetryList title="Поля не распознаются" items={qualityTelemetry.unrecognized_fields} />
          <TelemetryList title="Ломаются шаблоны" items={qualityTelemetry.broken_templates} />
          <TelemetryList title="Документы исключаются" items={qualityTelemetry.excluded_documents} />
        </div>
        <div className="compactList">
          {qualityTelemetry.suggestions.length === 0 && <small>Повторяющихся подтверждений для предложения нового правила пока нет.</small>}
          {qualityTelemetry.suggestions.map(item => <div key={item.suggestion_id}><span><b>{item.title}</b><small>{item.reason}</small></span><em>не включено</em></div>)}
        </div>
      </> : <small>Телеметрия ещё не загружена.</small>}
    </section>

    <section className="utilityCard advancedCard">
      <strong>Журнал действий</strong>
      <small>Цепочка записей защищена предыдущим и текущим SHA-256-хэшем.</small>
      <div className="compactList auditList">{audit.map(item => <div key={item.event_id}><span><b>{item.event_type}</b><small>{item.created_at} · {item.event_hash.slice(0, 12)}…</small></span></div>)}</div>
    </section>
  </div>;
}

function TelemetryList({ title, items }: { title: string; items: Array<{ key: string; count: number }> }) {
  return <div><b>{title}</b>{items.length === 0 ? <small>нет данных</small> : items.slice(0, 8).map(item => <span key={item.key}>{item.key} <strong>{item.count}</strong></span>)}</div>;
}

function componentStateLabel(state: SidecarToolStatus['state']) { return ({ bundled: 'встроен', downloaded: 'докачан и проверен', system: 'найден в системе', missing: 'не найден' } as const)[state]; }

function caseStatusLabel(status: string) {
  const labels: Record<string, string> = {
    received: 'получено', normalizing: 'нормализуется', recognizing: 'распознаётся', checking: 'проверяется',
    attention: 'требует внимания', ready: 'готово к генерации', generating: 'генерируется', publishing: 'публикуется',
    completed: 'завершено', failed: 'ошибка', cancelled: 'перезапущено',
  };
  return labels[status] || status;
}

function percent(value: number, total: number) { return total > 0 ? `${Math.round((value / total) * 100)}%` : '—'; }
function ratio(value:number,total:number){return total>0?(value/total).toFixed(1):'—'}
function formatMinutes(value:number){const minutes=Math.round(value);if(minutes<60)return `${minutes} мин`;const hours=Math.floor(minutes/60),rest=minutes%60;return rest?`${hours} ч ${rest} мин`:`${hours} ч`}
function exceptionCategoryLabel(category:string){if(category==='risk_gate')return 'Проверка данных';if(category==='bundle_decision')return 'Состав комплекта';return 'Требует внимания';}

function calendarHorizon(years:number[]){const current=new Date().getFullYear(),last=years.length?Math.max(...years):0,gap=last-current;if(gap>=2)return{urgent:false,message:`Календарь подтверждён до ${last} года и проверяется при запуске.`};if(gap>=1)return{urgent:false,message:`Календарь подтверждён только до ${last} года. Подготовьте подписанный пакет следующего года заранее.`};return{urgent:true,message:`Нет подтверждённого календаря после ${last||'текущего года'}. Расчёты будущих рабочих сроков будут остановлены до обновления календаря.`}}
function message(error: unknown) { return error instanceof Error ? error.message : typeof error === 'string' ? error : JSON.stringify(error); }
function safeSource(source: string) { const parts = source.replaceAll('\\', '/').split('/'); return parts.at(-1) || 'источник не указан'; }

function parseExceptionDetails(value: string): Record<string, unknown> | null {
  try { const parsed = JSON.parse(value); return parsed && typeof parsed === 'object' ? parsed as Record<string, unknown> : null; }
  catch { return null; }
}

function readFileBytes(file: File): Promise<Uint8Array> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error('Не удалось прочитать файл.'));
    reader.onload = () => {
      if (!(reader.result instanceof ArrayBuffer)) {
        reject(new Error('Файл прочитан в неподдерживаемом формате.'));
        return;
      }
      resolve(new Uint8Array(reader.result));
    };
    reader.readAsArrayBuffer(file);
  });
}
