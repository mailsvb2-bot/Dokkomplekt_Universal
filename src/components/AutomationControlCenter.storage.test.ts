import { describe, expect, it } from 'vitest';
import { formatRetentionDuration, loadOptionalTechnicalStorage } from './AutomationControlCenter';
import type { TechnicalStorageStatus } from '../lib/types';

describe('AutomationControlCenter optional technical storage telemetry', () => {
  it('preserves configured retention precision instead of rounding hours to days', () => {
    expect(formatRetentionDuration(3600)).toBe('1 ч.');
    expect(formatRetentionDuration(25 * 3600)).toBe('1 д. 1 ч.');
    expect(formatRetentionDuration(14 * 24 * 3600)).toBe('14 дн.');
  });

  it('returns telemetry when measurement succeeds', async () => {
    const value: TechnicalStorageStatus = {
      total_bytes: 12,
      retention_managed_bytes: 7,
      categories: [],
    };
    await expect(loadOptionalTechnicalStorage(async () => value)).resolves.toEqual(value);
  });

  it('degrades to null without rejecting the automation center refresh', async () => {
    await expect(loadOptionalTechnicalStorage(async () => {
      throw new Error('filesystem telemetry unavailable');
    })).resolves.toBeNull();
  });
});
