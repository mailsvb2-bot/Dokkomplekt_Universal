import { describe, expect, it } from 'vitest';
import { canonicalStorageFieldId } from './fieldAliases';

describe('shared storage field aliases', () => {
  it('canonicalizes legacy aliases while preserving custom ids', () => {
    expect(canonicalStorageFieldId(' patient.full_name ')).toBe('subject.name');
    expect(canonicalStorageFieldId('organization.inn')).toBe('org.inn');
    expect(canonicalStorageFieldId('legal.contract_number')).toBe('contract.number');
    expect(canonicalStorageFieldId('custom.case_owner')).toBe('custom.case_owner');
  });
});
