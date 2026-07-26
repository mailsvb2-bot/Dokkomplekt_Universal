import { useEffect, useRef, useState } from 'react';
import type { ThemeMode, ThemeState } from '../theme';
import { PRESETS } from '../theme';

const BASE_LABELS: Record<ThemeMode, string> = { light: 'Светлая', dark: 'Тёмная', warm: 'Тёплая' };

export function ThemeSwitcher(props: { theme: ThemeState; onChange: (next: ThemeState) => void }) {
  const [open, setOpen] = useState(false);
  const { theme, onChange } = props;
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onDocMouseDown(e: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') setOpen(false);
    }
    document.addEventListener('mousedown', onDocMouseDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDocMouseDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  return (
    <div className="themeSwitch" ref={rootRef}>
      <button
        type="button"
        className="iconButton"
        aria-label="Тема оформления"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <span className="themeDot" style={{ background: 'var(--accent)' }} aria-hidden="true" />
        <span className="themeDotLabel">Тема</span>
      </button>

      {open && (
        <div className="themePopover" role="dialog" aria-label="Настройка темы">
          <p className="themeHeading">Готовые темы</p>
          <div className="themePresets">
            {(Object.keys(PRESETS) as Array<keyof typeof PRESETS>).map((key) => {
              const p = PRESETS[key];
              const active = theme.preset === key;
              return (
                <button
                  key={key}
                  type="button"
                  className={active ? 'presetChip active' : 'presetChip'}
                  onClick={() => onChange({ preset: key, base: p.mode, accent: p.vars.accent })}
                >
                  <span className="presetSwatches">
                    <span style={{ background: p.vars.panel2 }} />
                    <span style={{ background: p.vars.accent }} />
                    <span style={{ background: p.vars.text2 }} />
                  </span>
                  <span className="presetName">{key} · {p.label}</span>
                </button>
              );
            })}
          </div>

          <p className="themeHeading">Свои цвета</p>
          <div className="themeCustom">
            <label className="colorRow">
              <span>Акцент</span>
              <input
                type="color"
                value={theme.accent}
                onChange={(e) => onChange({ preset: 'custom', base: theme.base, accent: e.target.value })}
              />
            </label>
            <div className="baseRow" role="group" aria-label="Основа темы">
              {(['light', 'dark', 'warm'] as ThemeMode[]).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  className={theme.preset === 'custom' && theme.base === mode ? 'baseChip active' : 'baseChip'}
                  onClick={() => onChange({ preset: 'custom', base: mode, accent: theme.accent })}
                >
                  {BASE_LABELS[mode]}
                </button>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
