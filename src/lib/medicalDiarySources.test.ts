import { describe, expect, it } from 'vitest';
import {
  isFinalMedicalDiaryText,
  medicalDiagnosisKey,
  medicalDiaryFileKey,
  safeSourceKey,
  uniqueMedicalDiaryTexts,
} from './medicalDiarySources';

describe('medical diary source key contract', () => {
  it('uses one canonical ICD key across diagnosis and file forms', () => {
    expect(medicalDiagnosisKey('F20.0 Шизофрения параноидная')).toBe('f200');
    expect(medicalDiagnosisKey('Диагноз F20 . 0, ремиссия')).toBe('f200');
    expect(medicalDiaryFileKey('Дневники F20 . 0 — вариант.docx')).toBe('f200');
    expect(medicalDiagnosisKey('F20 Шизофрения')).toBe('f20');
  });

  it('keeps non-ICD diagnosis keys bounded by the storage contract', () => {
    const longDiagnosis = `${'Очень-длинный-диагноз-Ё'.repeat(12)} хвост`;
    const key = medicalDiagnosisKey(longDiagnosis);
    expect([...key]).toHaveLength(160);
    expect(key).toBe(
      longDiagnosis.toLocaleLowerCase('ru-RU').replace(/ё/g, 'е').replace(/[^\p{L}\p{N}]+/gu, '').slice(0, 160),
    );
  });

  it('shares filename cleanup, final-role detection and text deduplication', () => {
    expect(safeSourceKey('Дневники ВЭ — Лёгкая депрессия с датами.docx')).toBe('вэлегкаядепрессиясдатами');
    expect(isFinalMedicalDiaryText('Итоговый F20.0.docx')).toBe(true);
    expect(isFinalMedicalDiaryText('Дневники F20.0.docx')).toBe(false);
    expect(uniqueMedicalDiaryTexts([' Текст ', 'текст', 'Другой'])).toEqual(['Текст', 'Другой']);
  });
});
