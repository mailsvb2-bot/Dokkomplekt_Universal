import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AppErrorBoundary } from './AppErrorBoundary';

function Broken(): never {
  throw new Error('render exploded');
}

describe('AppErrorBoundary', () => {
  it('shows a recoverable screen instead of a white window', () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    render(<AppErrorBoundary><Broken /></AppErrorBoundary>);
    expect(screen.getByRole('alert').textContent).toContain('Интерфейс восстановлен после ошибки');
    expect(screen.getByText('render exploded')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Перезапустить интерфейс' })).toBeTruthy();
  });
});
