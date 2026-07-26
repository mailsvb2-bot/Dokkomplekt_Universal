/**
 * Presentation-only theme system.
 *
 * No business logic lives here — this module only maps a chosen palette to CSS
 * custom properties on the document root. Three presets (A/B/C) plus a "custom"
 * mode where the user picks their own accent on top of a light/dark/warm base.
 */

export type ThemeMode = 'light' | 'dark' | 'warm';
export type PresetKey = 'A' | 'B' | 'C';

export interface ThemeVars {
  bg: string;
  frame: string;
  panel2: string;
  panel: string;
  line: string;
  accent: string;
  accentBg: string;
  accentTx: string;
  accentLine: string;
  text: string;
  text2: string;
  muted: string;
  field: string;
  fieldLine: string;
  dash: string;
  dropBg: string;
  mutedChip: string;
  paper: string;
  paperLine: string;
  lineBar: string;
  footBg: string;
  btnLine: string;
  onAccent: string;
  warn: string;
  ok: string;
  danger: string;
  radius: string;
  radiusCard: string;
}

export interface ThemeState {
  preset: PresetKey | 'custom';
  base: ThemeMode;
  accent: string;
}

export const PRESETS: Record<PresetKey, { label: string; mode: ThemeMode; vars: ThemeVars }> = {
  A: {
    label: 'Нейтральный светлый',
    mode: 'light',
    vars: {
      bg: '#F5F7FA', frame: '#F5F7FA', panel2: '#FFFFFF', panel: '#F7F9FB', line: '#EEF2F6',
      accent: '#534AB7', accentBg: '#EDECFB', accentTx: '#3B348A', accentLine: '#D3CEF3',
      text: '#0F1B2A', text2: '#5A6B7B', muted: '#94A3B4', field: '#FFFFFF', fieldLine: '#DDE5EE',
      dash: '#CBD6E2', dropBg: '#FBFDFE', mutedChip: '#F1F5F9', paper: '#FFFFFF', paperLine: '#E4EAF1',
      lineBar: '#E1E8F0', footBg: '#FCFDFE', btnLine: '#D7DFE9', onAccent: '#FFFFFF',
      warn: '#BA7517', ok: '#1D9E75', danger: '#C0392B', radius: '6px', radiusCard: '10px',
    },
  },
  B: {
    label: 'Тёмный сфокусированный',
    mode: 'dark',
    vars: {
      bg: '#0B0F14', frame: '#0B0F14', panel2: '#0F141A', panel: '#0D131A', line: '#1B242E',
      accent: '#1D9E75', accentBg: '#13322A', accentTx: '#5DCAA5', accentLine: '#204A3C',
      text: '#E6EDF3', text2: '#8A97A6', muted: '#63727F', field: '#111922', fieldLine: '#263340',
      dash: '#2A3946', dropBg: '#111922', mutedChip: '#1A2530', paper: '#0A0E13', paperLine: '#1E2833',
      lineBar: '#212C37', footBg: '#0D131A', btnLine: '#2A3946', onAccent: '#04241B',
      warn: '#EF9F27', ok: '#5DCAA5', danger: '#E24B4A', radius: '6px', radiusCard: '10px',
    },
  },
  C: {
    label: 'Тёплый спокойный',
    mode: 'warm',
    vars: {
      bg: '#F1EBE0', frame: '#F1EBE0', panel2: '#FBF8F2', panel: '#F4EEE3', line: '#EFE7D9',
      accent: '#0F6E56', accentBg: '#E1EEE7', accentTx: '#0C513F', accentLine: '#C6E0D5',
      text: '#2C2A25', text2: '#6B6455', muted: '#A99E8A', field: '#FFFFFF', fieldLine: '#E4DBC9',
      dash: '#D8CDB8', dropBg: '#FCFAF5', mutedChip: '#F0EADE', paper: '#FFFFFF', paperLine: '#EAE1D2',
      lineBar: '#EDE5D6', footBg: '#FBF8F2', btnLine: '#DDD3C0', onAccent: '#FFFFFF',
      warn: '#BA7517', ok: '#0F6E56', danger: '#B23A2E', radius: '9px', radiusCard: '12px',
    },
  },
};

const CSS_KEYS: Record<keyof ThemeVars, string> = {
  bg: '--bg', frame: '--frame', panel2: '--panel2', panel: '--panel', line: '--line',
  accent: '--accent', accentBg: '--accent-bg', accentTx: '--accent-tx', accentLine: '--accent-line',
  text: '--text', text2: '--text-2', muted: '--muted', field: '--field', fieldLine: '--field-line',
  dash: '--dash', dropBg: '--drop-bg', mutedChip: '--muted-chip', paper: '--paper', paperLine: '--paper-line',
  lineBar: '--line-bar', footBg: '--foot-bg', btnLine: '--btn-line', onAccent: '--on-accent',
  warn: '--warn', ok: '--ok', danger: '--danger', radius: '--radius', radiusCard: '--radius-card',
};

function clamp(n: number): number {
  return Math.max(0, Math.min(255, Math.round(n)));
}

function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace('#', '');
  const full = h.length === 3 ? h.split('').map((c) => c + c).join('') : h;
  return [parseInt(full.slice(0, 2), 16), parseInt(full.slice(2, 4), 16), parseInt(full.slice(4, 6), 16)];
}

function rgbToHex(r: number, g: number, b: number): string {
  return '#' + [r, g, b].map((v) => clamp(v).toString(16).padStart(2, '0')).join('');
}

function mix(a: string, b: string, t: number): string {
  const [r1, g1, b1] = hexToRgb(a);
  const [r2, g2, b2] = hexToRgb(b);
  return rgbToHex(r1 + (r2 - r1) * t, g1 + (g2 - g1) * t, b1 + (b2 - b1) * t);
}

function luminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex).map((v) => v / 255);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** Build a full palette from a base preset, optionally overriding the accent hue. */
export function buildTheme(state: ThemeState): ThemeVars {
  const presetKey: PresetKey = state.preset === 'custom'
    ? (state.base === 'dark' ? 'B' : state.base === 'warm' ? 'C' : 'A')
    : state.preset;
  const base = PRESETS[presetKey].vars;
  if (state.preset !== 'custom') return base;

  const accent = state.accent;
  const dark = state.base === 'dark';
  const accentBg = dark ? mix(accent, base.panel, 0.78) : mix(accent, base.panel2, 0.88);
  const accentTx = dark ? mix(accent, '#ffffff', 0.35) : mix(accent, '#000000', 0.32);
  const accentLine = dark ? mix(accent, base.panel, 0.55) : mix(accent, base.panel2, 0.62);
  const onAccent = luminance(accent) > 0.55 ? '#0b1410' : '#ffffff';
  return { ...base, accent, accentBg, accentTx, accentLine, onAccent, ok: accent };
}

export function applyTheme(vars: ThemeVars): void {
  const root = typeof document !== 'undefined' ? document.documentElement : null;
  if (!root) return;
  (Object.keys(CSS_KEYS) as Array<keyof ThemeVars>).forEach((k) => {
    root.style.setProperty(CSS_KEYS[k], vars[k]);
  });
  root.style.colorScheme = luminance(vars.bg) < 0.4 ? 'dark' : 'light';
}

const STORE_KEY = 'dokkomplekt.theme.v1';

export const DEFAULT_THEME: ThemeState = { preset: 'A', base: 'light', accent: PRESETS.A.vars.accent };

export function loadTheme(): ThemeState {
  try {
    const raw = typeof localStorage !== 'undefined' ? localStorage.getItem(STORE_KEY) : null;
    if (!raw) return DEFAULT_THEME;
    const parsed = JSON.parse(raw) as Partial<ThemeState>;
    return {
      preset: parsed.preset ?? 'A',
      base: parsed.base ?? 'light',
      accent: parsed.accent ?? PRESETS.A.vars.accent,
    };
  } catch {
    return DEFAULT_THEME;
  }
}

export function saveTheme(state: ThemeState): void {
  try {
    if (typeof localStorage !== 'undefined') localStorage.setItem(STORE_KEY, JSON.stringify(state));
  } catch {
    /* persistence is best-effort */
  }
}
