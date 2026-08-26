import React from 'react';
import ReactDOM from 'react-dom/client';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { App } from './App';
import { ensureOutputRoot, getDefaultOutputRoot } from './lib/api';
import { AppErrorBoundary } from './components/AppErrorBoundary';
import { ensureDefaultOutputRoot, getOutputRootBootstrapError } from './lib/outputRootBootstrap';
import './styles.css';

const READY_WINDOW_TITLE = 'Dokkomplekt Universal';
const RENDER_PROBE_INTERVAL_MS = 50;
const MAX_RENDER_PROBE_ATTEMPTS = 300;
const REQUIRED_STABLE_READY_CHECKS = 2;

function signalNativeWindowWhenRendered(root: HTMLElement): void {
  let remainingAttempts = MAX_RENDER_PROBE_ATTEMPTS;
  let stableReadyChecks = 0;

  const signalReady = (): void => {
    // Keep a browser-level title fallback and always attempt the native Tauri
    // title update. Browser-only tests may not expose Tauri internals, so the
    // synchronous call is guarded by try/catch instead of a private runtime flag.
    document.title = READY_WINDOW_TITLE;
    try {
      void getCurrentWindow()
        .setTitle(READY_WINDOW_TITLE)
        .catch((error: unknown) => {
          console.error('Failed to signal rendered native window', error);
        });
      void invoke('get_process_blueprints').catch((error: unknown) => {
        console.error('Failed to confirm rendered frontend IPC', error);
      });
    } catch (error: unknown) {
      console.error('Failed to access rendered native window', error);
    }
  };

  const probe = (): void => {
    const style = window.getComputedStyle(root);
    const hasRenderedContent =
      root.childElementCount > 0 &&
      (root.textContent?.trim().length ?? 0) > 0 &&
      style.display !== 'none' &&
      style.visibility !== 'hidden';

    stableReadyChecks = hasRenderedContent ? stableReadyChecks + 1 : 0;
    if (stableReadyChecks >= REQUIRED_STABLE_READY_CHECKS) {
      signalReady();
      return;
    }

    remainingAttempts -= 1;
    if (remainingAttempts > 0) {
      window.setTimeout(probe, RENDER_PROBE_INTERVAL_MS);
      return;
    }

    console.error('Rendered React root did not become ready for native window signal');
  };

  // WebKitGTK can throttle requestAnimationFrame under Xvfb even while its native
  // window is mapped. Timer-driven consecutive checks prove a committed, stable,
  // visible React tree without relying on compositor paint callbacks.
  window.setTimeout(probe, 0);
}

const rootElement = document.getElementById('root');
if (!(rootElement instanceof HTMLElement)) {
  throw new Error('Application root element is missing');
}

async function bootstrapApplication(root: HTMLElement): Promise<void> {
  // Resolve and physically create the canonical first-run Desktop output before
  // App's synchronous useState(loadOutputRoot) executes. Never hide a filesystem
  // refusal: the donor applications made folder failures explicit, and Universal
  // must preserve that user-visible guarantee.
  await ensureDefaultOutputRoot(localStorage, getDefaultOutputRoot, ensureOutputRoot);
  const outputRootBootstrapError = getOutputRootBootstrapError();

  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <AppErrorBoundary>
        {outputRootBootstrapError ? (
          <section className="startupRecovery" role="alert" aria-label="Не удалось подготовить папку готовых документов">
            <div>
              <strong>Папка готовых документов не подготовлена</strong>
              <span>Программа не будет считать путь рабочим молча. Проверьте доступ к рабочему столу или выберите другую папку в настройках.</span>
              <small>{outputRootBootstrapError}</small>
            </div>
          </section>
        ) : null}
        <App />
      </AppErrorBoundary>
    </React.StrictMode>
  );

  signalNativeWindowWhenRendered(root);
}

void bootstrapApplication(rootElement);
