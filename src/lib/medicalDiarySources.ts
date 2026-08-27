export const MEDICAL_DIARY_REGULAR_PREFIX = 'professional.medical.diary.regular.';
export const MEDICAL_DIARY_FINAL_PREFIX = 'professional.medical.diary.final.';

export function safeSourceKey(value: string): string {
  return value
    .replace(/\.[^.]+$/, '')
    .toLocaleLowerCase('ru-RU')
    .replace(/ё/g, 'е')
    .replace(/^(?:дневники?|дневниковые|тексты?|даты|шаблоны?)[\s._—–:;,-]*/u, '')
    .replace(/[^\p{L}\p{N}]+/gu, '')
    .slice(0, 96);
}

function medicalIcdCodeKey(value: string): string {
  const folded = value.toLocaleUpperCase('ru-RU').replace(/Ё/g, 'Е');
  const icdCode = folded.match(/(?:^|[^\p{L}\p{N}])([A-Z]\s*\d{2}(?:\s*\.\s*\d{1,4})?)(?=$|[^\p{L}\p{N}])/u)?.[1];
  return icdCode
    ? icdCode.toLocaleLowerCase('ru-RU').replace(/[^\p{L}\p{N}]+/gu, '').slice(0, 32)
    : '';
}

export function medicalDiagnosisKey(value: string): string {
  const codeKey = medicalIcdCodeKey(value);
  if (codeKey) return codeKey;
  return value
    .toLocaleLowerCase('ru-RU')
    .replace(/ё/g, 'е')
    .replace(/[^\p{L}\p{N}]+/gu, '')
    .slice(0, 160);
}

export function medicalDiaryFileKey(fileName: string): string {
  const stem = fileName.replace(/\.[^.]+$/, '');
  return medicalIcdCodeKey(stem) || safeSourceKey(fileName);
}

export function isFinalMedicalDiaryText(fileName: string): boolean {
  const name = fileName.toLocaleLowerCase('ru-RU').replace(/ё/g, 'е');
  return /(?:финал|итог|выписк|заключитель)/u.test(name);
}

export function uniqueMedicalDiaryTexts(values: string[]): string[] {
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
