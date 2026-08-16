import { useMemo, useState, type ChangeEvent, type DragEvent } from 'react';
import type { DocumentTemplateSpec, DomainKind } from '../lib/types';
import {
  deleteClauseBlock,
  importLearningExampleFile,
  importTemplateFile,
  listClauseBlocks,
  saveClauseBlock,
} from '../lib/api';
import { arrayBufferToBase64, readFileBytes } from '../lib/appSupport';
import { MEDICAL_PROFILE_QUICK_OPTION_PRESETS } from '../data/medicalProfilePresets';

const MATERIAL_INDEX_BLOCK = 'professional.materials.index';
const MEDICAL_DATE_TEMPLATES_BLOCK = 'professional.medical.diary.date_templates';
const MEDICAL_RVK_OPTIONS_BLOCK = 'professional.medical.rvk.quick_options';
const MEDICAL_DIARY_REGULAR_PREFIX = 'professional.medical.diary.regular.';
const MEDICAL_DIARY_FINAL_PREFIX = 'professional.medical.diary.final.';

interface MaterialIndexEntry {
  block_id: string;
  file_name: string;
  domain: string;
  imported_at: string;
}

interface DiaryTemplateEntry {
  file_name: string;
  source_path: string;
}

interface DroppedFileEntry {
  isFile: boolean;
  isDirectory: boolean;
  file(callback: (file: File) => void, error?: (error: unknown) => void): void;
  createReader(): { readEntries(callback: (entries: DroppedFileEntry[]) => void, error?: (error: unknown) => void): void };
}

async function filesFromDroppedEntry(entry: DroppedFileEntry): Promise<File[]> {
  if (entry.isFile) {
    return new Promise((resolve, reject) => entry.file(file => resolve([file]), reject));
  }
  if (!entry.isDirectory) return [];
  const reader = entry.createReader();
  const children: DroppedFileEntry[] = [];
  for (;;) {
    const batch = await new Promise<DroppedFileEntry[]>((resolve, reject) => reader.readEntries(resolve, reject));
    if (!batch.length) break;
    children.push(...batch);
  }
  return (await Promise.all(children.map(filesFromDroppedEntry))).flat();
}

async function filesFromDrop(event: DragEvent<HTMLDivElement>): Promise<File[]> {
  const items = Array.from(event.dataTransfer.items ?? []);
  const entries = items
    .map(item => (item as unknown as { webkitGetAsEntry?: () => DroppedFileEntry | null }).webkitGetAsEntry?.())
    .filter((entry): entry is DroppedFileEntry => Boolean(entry));
  if (entries.length) return (await Promise.all(entries.map(filesFromDroppedEntry))).flat();
  return Array.from(event.dataTransfer.files ?? []);
}

function domainKey(domain: DomainKind): string {
  if (typeof domain === 'object') return `custom-${safeKey(domain.Custom) || 'profile'}`;
  return domain.toLowerCase();
}

export function safeKey(value: string): string {
  return value
    .replace(/\.[^.]+$/, '')
    .toLocaleLowerCase('ru-RU')
    .replace(/ё/g, 'е')
    .replace(/\b(?:дневник|дневники|дневниковые|текст|тексты|даты|шаблон|шаблоны)\b/gu, ' ')
    .replace(/[^\p{L}\p{N}]+/gu, '')
    .slice(0, 96);
}

function isDiaryRole(roleId: string): boolean {
  const role = roleId.trim().toLowerCase();
  return role === 'diary' || role === 'diaries' || role.endsWith('.diary') || role.endsWith('.diaries');
}

function isRvkRole(roleId: string): boolean {
  const role = roleId.trim().toLowerCase();
  return role === 'rvk_act' || role.endsWith('.rvk_act') || role.includes('rvk') || role.includes('рвк');
}

function isFinalDiaryText(fileName: string): boolean {
  const name = fileName.toLocaleLowerCase('ru-RU').replace(/ё/g, 'е');
  return /(?:финал|итог|выписк|заключитель)/u.test(name);
}

function uniqueTexts(values: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of values) {
    const value = raw.trim();
    const key = value.toLocaleLowerCase('ru-RU').replace(/\s+/g, ' ');
    if (!value || seen.has(key)) continue;
    seen.add(key);
    out.push(value);
  }
  return out;
}

export function AdditionalMaterialsPanel(props: {
  documents: DocumentTemplateSpec[];
  selectedDocumentIds: string[];
  busy: boolean;
}) {
  const [status, setStatus] = useState('');
  const [working, setWorking] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [customRvk, setCustomRvk] = useState('');
  const selected = useMemo(
    () => props.documents.filter(document => props.selectedDocumentIds.includes(document.id)),
    [props.documents, props.selectedDocumentIds],
  );
  const medicalDiarySelected = selected.some(document => document.category === 'Medical' && isDiaryRole(document.role_id));
  const medicalRvkSelected = selected.some(document => document.category === 'Medical' && isRvkRole(document.role_id));
  const domains = useMemo(() => {
    const values = new Map<string, DomainKind>();
    for (const document of selected) values.set(domainKey(document.category), document.category);
    return [...values.values()];
  }, [selected]);

  if (!selected.length) return null;

  async function withWork<T>(label: string, action: () => Promise<T>): Promise<T | null> {
    setWorking(true);
    setStatus(label);
    try {
      return await action();
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error));
      return null;
    } finally {
      setWorking(false);
    }
  }

  async function extractMaterial(file: File) {
    const bytes = await readFileBytes(file);
    return importLearningExampleFile(file.name, arrayBufferToBase64(bytes));
  }

  async function saveMaterialIndex(newEntries: MaterialIndexEntry[]) {
    const blocks = await listClauseBlocks();
    const existing = blocks.find(block => block.block_id === MATERIAL_INDEX_BLOCK)?.content;
    let current: MaterialIndexEntry[] = [];
    if (existing) {
      try {
        const parsed = JSON.parse(existing);
        if (Array.isArray(parsed)) current = parsed as MaterialIndexEntry[];
      } catch { /* replace invalid old index with a valid one */ }
    }
    const byId = new Map(current.map(entry => [entry.block_id, entry]));
    for (const entry of newEntries) byId.set(entry.block_id, entry);
    await saveClauseBlock(MATERIAL_INDEX_BLOCK, 'Дополнительные материалы · индекс', JSON.stringify([...byId.values()], null, 2));
  }

  async function importGenericFiles(files: File[]) {
    if (!files.length) return;
    await withWork('Импортируем дополнительные материалы…', async () => {
      const indexEntries: MaterialIndexEntry[] = [];
      for (const file of files) {
        const imported = await extractMaterial(file);
        const content = imported.extracted_text.trim();
        if (!content) continue;
        for (const domain of domains.length ? domains : ['Generic' as DomainKind]) {
          const blockId = `professional.material.${domainKey(domain)}.${safeKey(file.name) || 'source'}`;
          await saveClauseBlock(blockId, `Дополнительный материал: ${file.name}`, content);
          indexEntries.push({
            block_id: blockId,
            file_name: file.name,
            domain: domainKey(domain),
            imported_at: new Date().toISOString(),
          });
        }
      }
      await saveMaterialIndex(indexEntries);
      setStatus(`Дополнительные материалы сохранены: ${indexEntries.length}. Они доступны выбранным профессиональным профилям как specialist-owned blocks.`);
    });
  }

  async function importDiaryTexts(files: File[]) {
    if (!files.length) return;
    await withWork('Импортируем медицинскую библиотеку текстов…', async () => {
      const existing = await listClauseBlocks();
      const existingById = new Map(existing.map(block => [block.block_id, block.content]));
      const grouped = new Map<string, string[]>();
      for (const file of files) {
        const imported = await extractMaterial(file);
        const content = imported.extracted_text.trim();
        if (!content) continue;
        const key = safeKey(file.name);
        if (!key) continue;
        const prefix = isFinalDiaryText(file.name) ? MEDICAL_DIARY_FINAL_PREFIX : MEDICAL_DIARY_REGULAR_PREFIX;
        const blockId = `${prefix}${key}`;
        const bucket = grouped.get(blockId) ?? [];
        bucket.push(content);
        grouped.set(blockId, bucket);
      }
      for (const [blockId, values] of grouped) {
        const previous = existingById.get(blockId);
        const merged = uniqueTexts([...(previous ? [previous] : []), ...values]).join('\n\n');
        await saveClauseBlock(blockId, `Медицинские дневники: ${blockId.split('.').pop()}`, merged);
      }
      setStatus(`Библиотека «Тексты» обновлена: ${grouped.size} диагноз-ориентированных источник(а). Медицинский генератор использует их без выдумывания текста.`);
    });
  }

  async function importDiaryDateTemplates(files: File[]) {
    const wordFiles = files.filter(file => /\.doc[mx]$/i.test(file.name));
    if (!wordFiles.length) {
      setStatus('Для «Даты» выберите DOCX/DOCM-файлы с номерами 01–31.');
      return;
    }
    await withWork('Импортируем шаблоны «Даты»…', async () => {
      const entries: DiaryTemplateEntry[] = [];
      for (const file of wordFiles) {
        const bytes = await readFileBytes(file);
        const imported = await importTemplateFile(
          `medical_diary_date_${safeKey(file.name) || Date.now()}`,
          { fileName: file.name, bytesBase64: arrayBufferToBase64(bytes) },
        );
        entries.push({ file_name: file.name, source_path: imported.template_path });
      }
      await saveClauseBlock(
        MEDICAL_DATE_TEMPLATES_BLOCK,
        'Медицинские дневники · шаблоны дат 01–31',
        JSON.stringify(entries, null, 2),
      );
      setStatus(`«Даты» сохранены: ${entries.length}. При создании дневников backend выберет номер по дню поступления (с совместимым D0+1 fallback).`);
    });
  }

  async function applyRvkPreset(presetId: string) {
    const preset = MEDICAL_PROFILE_QUICK_OPTION_PRESETS.find(item => item.id === presetId);
    if (!preset) return;
    await withWork('Сохраняем быстрые варианты РВК…', async () => {
      await saveClauseBlock(
        MEDICAL_RVK_OPTIONS_BLOCK,
        `РВК · ${preset.title}`,
        JSON.stringify(preset.rvkCommissariats, null, 2),
      );
      setStatus(`Быстрые варианты РВК сохранены для медицинского профиля: ${preset.rvkCommissariats.join(' · ')}. Ручной ввод остаётся доступен.`);
    });
  }

  async function saveCustomRvkOptions(raw: string) {
    const options = uniqueTexts(raw.split(/[\n;]/));
    if (!options.length) {
      await deleteClauseBlock(MEDICAL_RVK_OPTIONS_BLOCK);
      setStatus('Профильные быстрые варианты РВК очищены; остаётся ручной ввод.');
      return;
    }
    await withWork('Сохраняем свои варианты РВК…', async () => {
      await saveClauseBlock(MEDICAL_RVK_OPTIONS_BLOCK, 'РВК · свои варианты', JSON.stringify(options, null, 2));
      setStatus(`Сохранено вариантов РВК: ${options.length}.`);
    });
  }

  function filesFrom(event: ChangeEvent<HTMLInputElement>): File[] {
    const files = Array.from(event.currentTarget.files ?? []);
    event.currentTarget.value = '';
    return files;
  }

  return (
    <section className="additionalMaterialsPanel" aria-label="Дополнительные источники и материалы">
      <div className="additionalMaterialsHeading">
        <div>
          <strong>Дополнительные источники / материалы</strong>
          <small>Основной документ остаётся главным. Эти материалы дополняют выбранный профессиональный профиль и не создают отдельный «второй мозг».</small>
        </div>
      </div>

      <div
        className={`additionalMaterialsDropZone ${dragging ? 'dragging' : ''}`}
        onDragEnter={(event: DragEvent<HTMLDivElement>) => { event.preventDefault(); setDragging(true); }}
        onDragOver={(event: DragEvent<HTMLDivElement>) => event.preventDefault()}
        onDragLeave={() => setDragging(false)}
        onDrop={(event: DragEvent<HTMLDivElement>) => {
          event.preventDefault();
          setDragging(false);
          void filesFromDrop(event).then(files => { if (files.length) return importGenericFiles(files); });
        }}
      >
        Перетащите сюда дополнительные файлы или содержимое папки
      </div>

      <div className="additionalMaterialActions">
        <label className="softBtn fileBtn">
          <i className="ti ti-files" aria-hidden="true" /> Добавить файлы
          <input
            type="file"
            multiple
            accept=".docx,.docm,.doc,.pdf,.txt,.md,.rtf,.csv,.xlsx,.xls,.ods,.odt,.png,.jpg,.jpeg,.tif,.tiff,.bmp,.webp,.eml,.msg,.zip,.7z,.rar"
            onChange={(event) => { void importGenericFiles(filesFrom(event)); }}
            disabled={working || props.busy}
            style={{ display: 'none' }}
          />
        </label>
        <label className="softBtn fileBtn">
          <i className="ti ti-folder-plus" aria-hidden="true" /> Добавить папку
          <input
            type="file"
            multiple
            onChange={(event) => { void importGenericFiles(filesFrom(event)); }}
            disabled={working || props.busy}
            style={{ display: 'none' }}
            {...({ webkitdirectory: '', directory: '' } as Record<string, string>)}
          />
        </label>
      </div>

      {medicalDiarySelected && (
        <section className="medicalAdditionalSources" aria-label="Медицинские дневники">
          <div>
            <strong>Медицинские дневники</strong>
            <small>Эти источники видит только медицинский профиль при выбранной роли дневников.</small>
          </div>
          <div className="medicalSourceButtons">
            <div className="medicalSourceChoice">
              <label className="primaryBtn fileBtn">
                <i className="ti ti-calendar" aria-hidden="true" /> Даты
                <input
                  type="file"
                  multiple
                  accept=".docx,.docm"
                  onChange={(event) => { void importDiaryDateTemplates(filesFrom(event)); }}
                  disabled={working || props.busy}
                  style={{ display: 'none' }}
                  {...({ webkitdirectory: '', directory: '' } as Record<string, string>)}
                />
              </label>
              <label className="textBtn fileBtn">
                выбрать отдельные файлы
                <input type="file" multiple accept=".docx,.docm" onChange={(event) => { void importDiaryDateTemplates(filesFrom(event)); }} disabled={working || props.busy} style={{ display: 'none' }} />
              </label>
              <small>Папка 01–31 или отдельные DOCX/DOCM. Backend сам выберет нужный дневниковый шаблон по дате поступления.</small>
            </div>
            <div className="medicalSourceChoice">
              <label className="primaryBtn fileBtn">
                <i className="ti ti-notes" aria-hidden="true" /> Тексты
                <input
                  type="file"
                  multiple
                  accept=".docx,.docm,.doc,.txt,.rtf,.odt,.pdf"
                  onChange={(event) => { void importDiaryTexts(filesFrom(event)); }}
                  disabled={working || props.busy}
                  style={{ display: 'none' }}
                  {...({ webkitdirectory: '', directory: '' } as Record<string, string>)}
                />
              </label>
              <label className="textBtn fileBtn">
                выбрать отдельные файлы
                <input type="file" multiple accept=".docx,.docm,.doc,.txt,.rtf,.odt,.pdf" onChange={(event) => { void importDiaryTexts(filesFrom(event)); }} disabled={working || props.busy} style={{ display: 'none' }} />
              </label>
              <small>Библиотека текстов сопоставляется с диагнозом лексическим matcher-ом; профессиональные синонимы находятся только в medical data pack.</small>
            </div>
          </div>
        </section>
      )}

      {medicalRvkSelected && (
        <details className="medicalProfileOptions">
          <summary>Быстрые варианты Акта РВК</summary>
          <p>Названия не зашиты в Universal Core: это настройка медицинского профиля.</p>
          <div className="additionalMaterialActions">
            {MEDICAL_PROFILE_QUICK_OPTION_PRESETS.map(preset => (
              <button key={preset.id} type="button" className="softBtn" disabled={working || props.busy} onClick={() => { void applyRvkPreset(preset.id); }}>
                {preset.title}
              </button>
            ))}
          </div>
          <label className="profileOptionsEditor">
            <span>Свои варианты — по одному в строке</span>
            <textarea value={customRvk} onChange={event => setCustomRvk(event.target.value)} placeholder={'Военкомат 1\nВоенкомат 2'} />
          </label>
          <button type="button" className="softBtn" disabled={working || props.busy} onClick={() => { void saveCustomRvkOptions(customRvk); }}>Сохранить свои варианты</button>
        </details>
      )}

      {status && <p className="additionalMaterialsStatus" role="status">{status}</p>}
    </section>
  );
}
