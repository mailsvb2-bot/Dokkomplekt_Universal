import { describe, expect, it } from 'vitest';
import { normalizeLegacyTextFileBytes } from './legacyTextEncoding';

describe('donor legacy text encoding', () => {
  it('normalizes Windows-1251 Russian TXT to UTF-8', () => {
    const bytes = Uint8Array.from([
      0xD1, 0xEE, 0xF1, 0xF2, 0xEE, 0xFF, 0xED, 0xE8, 0xE5, 0x20, 0xEF, 0xE0, 0xF6,
      0xE8, 0xE5, 0xED, 0xF2, 0xE0, 0x20, 0xF1, 0xF2, 0xE0, 0xE1, 0xE8, 0xEB, 0xFC, 0xED,
      0xEE, 0xE5, 0x2E,
    ]);
    const normalized = normalizeLegacyTextFileBytes('F20.0.txt', bytes.buffer);
    expect(new TextDecoder().decode(normalized)).toBe('Состояние пациента стабильное.');
  });

  it('preserves valid UTF-8 and non-text binaries', () => {
    const utf8 = new TextEncoder().encode('Дневник UTF-8').buffer as ArrayBuffer;
    expect(new TextDecoder().decode(normalizeLegacyTextFileBytes('status.txt', utf8))).toBe('Дневник UTF-8');
    const docx = Uint8Array.from([0x50, 0x4b, 0x03, 0x04]).buffer;
    expect([...new Uint8Array(normalizeLegacyTextFileBytes('template.docx', docx))]).toEqual([0x50, 0x4b, 0x03, 0x04]);
  });

  it('preserves western Windows-1252 when Cyrillic evidence is absent', () => {
    const bytes = Uint8Array.from([0x63, 0x61, 0x66, 0xe9]);
    const normalized = normalizeLegacyTextFileBytes('notes.txt', bytes.buffer);
    expect(new TextDecoder().decode(normalized)).toBe('café');
  });
});
