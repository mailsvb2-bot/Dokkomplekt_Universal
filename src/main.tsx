import React from 'react';
import ReactDOM from 'react-dom/client';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { App } from './App';
import { AppErrorBoundary } from './components/AppErrorBoundary';
import './styles.css';

const READY_WINDOW_TITLE = 'Dokkomplekt Universal';
const MIN_RENDER_WIDTH = 800;
const MIN_RENDER_HEIGHT = 500;
const MAX_RENDER_PROBE_FRAMES = 120;

type TauriWindow = Window & { __TAURI_INTERNALS__?: unknown };

function signalNativeWindowWhenRendered(root: HTMLElement): void {
  let remainingFrames = MAX_RENDER_PROBE_FRAMES;

  const probe = (): void => {
    const rectangle = root.getBoundingClientRect();
    const style = window.getComputedStyle(root);
    const hasRenderedContent =
      root.childElementCount > 0 &&
      (root.textContent?.trim().length ?? 0) > 0 &&
      rectangle.width >= MIN_RENDER_WIDTH &&
      rectangle.height >= MIN_RENDER_HEIGHT &&
      style.display !== 'none' &&
      style.visibility !== 'hidden';

    if (hasRenderedContent) {
      if ((window as TauriWindow).__TAURI_INTERNALS__) {
        void getCurrentWindow()
          .setTitle(READY_WINDOW_TITLE)
          .catch(() => { /* Browser tests do not expose a native Tauri window. */ });
      }
      return;
    }

    remainingFrames -= 1;
    if (remainingFrames > 0) window.requestAnimationFrame(probe);
  };

  // A native ready title is emitted only after React has committed and the
  // browser completed two paint opportunities with a visible, non-empty root.
  window.requestAnimationFrame(() => window.requestAnimationFrame(probe));
}

const rootElement = document.getElementById('root');
if (!(rootElement instanceof HTMLElement)) {
  throw new Error('Application root element is missing');
}

ReactDOM.createRoot(rootElement).render(
  <React.StrictMode>
    <AppErrorBoundary>
      <App />
    </AppErrorBoundary>
  </React.StrictMode>
);

signalNativeWindowWhenRendered(rootElement);
