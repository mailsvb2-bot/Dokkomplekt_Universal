import aliases from '../../shared/field_aliases.json';

const STORAGE_FIELD_ALIASES = aliases as Record<string, string>;

/**
 * Canonical semantic storage id owned by the shared alias resource consumed by
 * both the Rust core and the thin UI. Unknown/custom ids remain unchanged.
 */
export function canonicalStorageFieldId(raw: string): string {
  const field = raw.trim();
  return STORAGE_FIELD_ALIASES[field] ?? field;
}
