const LEGACY_TEXT_EXTENSIONS = new Set(['txt', 'csv', 'tsv', 'md']);

/**
 * Normalize donor-era single-byte text files to UTF-8 before they reach the
 * canonical intake backend. Valid UTF-8 stays byte-for-byte unchanged.
 */
export function normalizeLegacyTextFileBytes(fileName: string, buffer: ArrayBuffer): ArrayBuffer {
  const extension = fileName.split('.').pop()?.trim().toLowerCase() ?? '';
  if (!LEGACY_TEXT_EXTENSIONS.has(extension) || buffer.byteLength === 0) return buffer;

  try {
    new TextDecoder('utf-8', { fatal: true }).decode(buffer);
    return buffer;
  } catch {
    // Continue with donor-era Windows code pages.
  }

  try {
    const cp1251 = new TextDecoder('windows-1251').decode(buffer);
    const cp1252 = new TextDecoder('windows-1252').decode(buffer);
    const cyrillic = [...cp1251].filter(character => /[\u0400-\u052f]/u.test(character)).length;
    const latin = [...cp1251].filter(character => /[A-Za-z]/u.test(character)).length;
    const decoded = cyrillic >= 2 && cyrillic > latin ? cp1251 : cp1252;
    return new TextEncoder().encode(decoded).buffer as ArrayBuffer;
  } catch {
    return buffer;
  }
}
