import { describe, expect, it } from 'vitest';
import { loadOptionalTechnicalStorage } from './AutomationControlCenter';
import type { TechnicalStorageStatus } from '../lib/types';

describe('AutomationControlCenter optional technical storage telemetry', () => {
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
