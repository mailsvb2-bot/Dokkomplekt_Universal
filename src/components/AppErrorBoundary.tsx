import React from 'react';

interface State {
  error: Error | null;
}

export class AppErrorBoundary extends React.Component<React.PropsWithChildren, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo): void {
    console.error('Dokkomplekt UI render failure', error, info);
  }

  render(): React.ReactNode {
    if (!this.state.error) return this.props.children;
    return (
      <main className="fatalError" role="alert">
        <h1>Интерфейс восстановлен после ошибки</h1>
        <p>Документы и настройки не удалены. Перезапустите только окно программы.</p>
        <details>
          <summary>Техническая информация</summary>
          <pre>{this.state.error.message}</pre>
        </details>
        <button type="button" onClick={() => globalThis.location.reload()}>
          Перезапустить интерфейс
        </button>
      </main>
    );
  }
}
