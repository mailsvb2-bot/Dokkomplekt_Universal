import { describe, expect, it, vi } from 'vitest';
import { closeGuidedScannerSafely } from './guidedScannerShutdown';

const scanner = {
  capture: null,
  session: { session_id: 'scan-1' },
  target: { mode: 'template' },
};

describe('closeGuidedScannerSafely', () => {
  it('keeps the scanner open if backend refuses to close Word', async () => {
    const close = vi.fn(async (_id: string, _discard: boolean, onError: (detail: string) => void) => {
      onError('Word process locked');
      return undefined;
    });
    const status = vi.fn();
    expect(await closeGuidedScannerSafely(scanner, close, status)).toBe(false);
    expect(close).toHaveBeenCalledWith('scan-1', true, expect.any(Function));
    expect(status).toHaveBeenCalledWith(expect.stringContaining('Word process locked'));
  });

  it('keeps the scanner open on a false response or thrown shutdown error', async () => {
    const status = vi.fn();
    expect(await closeGuidedScannerSafely(scanner, async () => false, status)).toBe(false);
    expect(await closeGuidedScannerSafely(scanner, async () => { throw Error('OS denied'); }, status)).toBe(false);
    expect(status).toHaveBeenLastCalledWith(expect.stringContaining('OS denied'));
  });

  it('does not redundantly close Word when capture proves it already closed', async () => {
    const close = vi.fn(async () => true);
    expect(await closeGuidedScannerSafely({ ...scanner, capture: { document_closed: true } }, close, vi.fn())).toBe(true);
    expect(close).not.toHaveBeenCalled();
  });

  it('reports success only for a successful shutdown response', async () => {
    const status = vi.fn();
    expect(await closeGuidedScannerSafely(scanner, async () => true, status)).toBe(true);
    expect(status).not.toHaveBeenCalled();
  });
});
