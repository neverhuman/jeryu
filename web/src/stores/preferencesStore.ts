// preferencesStore.ts — user UI preferences (W-FE-05 + W-CC-01).
//
// State persists to `localStorage` as `jeryu.preferences.v1`. The version
// suffix lets us migrate the schema later without poisoning user data.

import { create } from 'zustand';

export type ThemePreference = 'system' | 'light' | 'dark' | 'high-contrast';
export type DensityPreference = 'comfortable' | 'compact' | 'ultra-compact';
export type KeyboardMode = 'default' | 'vim';
export type DateFormat = 'relative' | 'iso' | 'long';
export type ReposViewMode = 'card' | 'table';
export type DiffMode = 'unified' | 'split';

export interface PreferencesState {
  theme: ThemePreference;
  density: DensityPreference;
  codeFontSize: number;
  dateFormat: DateFormat;
  keyboardMode: KeyboardMode;
  reposView: ReposViewMode;
  diffMode: DiffMode;
  /**
   * ISO timestamp of the most recent `mark-all-as-read` action on the
   * notifications inbox (W-FE-18). Events older than this are considered
   * "read" when computing the bell badge unread count.
   */
  notificationsLastSeen: string | null;
  setTheme: (theme: ThemePreference) => void;
  setDensity: (density: DensityPreference) => void;
  setCodeFontSize: (size: number) => void;
  setDateFormat: (format: DateFormat) => void;
  setKeyboardMode: (mode: KeyboardMode) => void;
  setReposView: (view: ReposViewMode) => void;
  setDiffMode: (mode: DiffMode) => void;
  /** Stamp `notificationsLastSeen` to `now` (or a caller-supplied ISO). */
  markNotificationsSeen: (at?: string) => void;
  reset: () => void;
}

const STORAGE_KEY = 'jeryu.preferences.v1';

const DEFAULTS: Pick<
  PreferencesState,
  | 'theme'
  | 'density'
  | 'codeFontSize'
  | 'dateFormat'
  | 'keyboardMode'
  | 'reposView'
  | 'diffMode'
  | 'notificationsLastSeen'
> = {
  theme: 'system',
  density: 'comfortable',
  codeFontSize: 13,
  dateFormat: 'relative',
  keyboardMode: 'default',
  reposView: 'card',
  diffMode: 'unified',
  notificationsLastSeen: null,
};

function loadInitial(): typeof DEFAULTS {
  if (typeof window === 'undefined') return DEFAULTS;
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULTS;
    // localStorage is attacker-/corruption-prone, so the parsed value stays
    // `unknown`: we read each field through `field()` and run it past a
    // per-field validator before it can reach the store. Nothing is trusted
    // on the strength of the cast alone.
    const parsed: unknown = JSON.parse(raw);
    const field = (key: keyof typeof DEFAULTS): unknown =>
      typeof parsed === 'object' && parsed !== null
        ? (parsed as Record<string, unknown>)[key]
        : undefined;
    const codeFontSize = field('codeFontSize');
    return {
      theme: validateTheme(field('theme')) ?? DEFAULTS.theme,
      density: validateDensity(field('density')) ?? DEFAULTS.density,
      codeFontSize:
        typeof codeFontSize === 'number' &&
        codeFontSize >= 10 &&
        codeFontSize <= 24
          ? codeFontSize
          : DEFAULTS.codeFontSize,
      dateFormat: validateDateFormat(field('dateFormat')) ?? DEFAULTS.dateFormat,
      keyboardMode:
        validateKeyboardMode(field('keyboardMode')) ?? DEFAULTS.keyboardMode,
      reposView: validateReposView(field('reposView')) ?? DEFAULTS.reposView,
      diffMode: validateDiffMode(field('diffMode')) ?? DEFAULTS.diffMode,
      notificationsLastSeen:
        validateIsoTimestamp(field('notificationsLastSeen')) ??
        DEFAULTS.notificationsLastSeen,
    };
  } catch {
    return DEFAULTS;
  }
}

function persist(state: typeof DEFAULTS): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // localStorage may be full or disabled (private mode). Silent.
  }
}

function validateTheme(input: unknown): ThemePreference | null {
  return input === 'system' ||
    input === 'light' ||
    input === 'dark' ||
    input === 'high-contrast'
    ? input
    : null;
}

function validateDensity(input: unknown): DensityPreference | null {
  return input === 'comfortable' ||
    input === 'compact' ||
    input === 'ultra-compact'
    ? input
    : null;
}

function validateDateFormat(input: unknown): DateFormat | null {
  return input === 'relative' || input === 'iso' || input === 'long'
    ? input
    : null;
}

function validateKeyboardMode(input: unknown): KeyboardMode | null {
  return input === 'default' || input === 'vim' ? input : null;
}

function validateReposView(input: unknown): ReposViewMode | null {
  return input === 'card' || input === 'table' ? input : null;
}

function validateDiffMode(input: unknown): DiffMode | null {
  return input === 'unified' || input === 'split' ? input : null;
}

function validateIsoTimestamp(input: unknown): string | null {
  if (typeof input !== 'string') return null;
  const ms = Date.parse(input);
  return Number.isFinite(ms) ? input : null;
}

export const usePreferencesStore = create<PreferencesState>((set, get) => {
  const initial = loadInitial();
  return {
    ...initial,
    setTheme: (theme) => {
      set({ theme });
      persistFromState(get);
    },
    setDensity: (density) => {
      set({ density });
      persistFromState(get);
    },
    setCodeFontSize: (codeFontSize) => {
      set({ codeFontSize });
      persistFromState(get);
    },
    setDateFormat: (dateFormat) => {
      set({ dateFormat });
      persistFromState(get);
    },
    setKeyboardMode: (keyboardMode) => {
      set({ keyboardMode });
      persistFromState(get);
    },
    setReposView: (reposView) => {
      set({ reposView });
      persistFromState(get);
    },
    setDiffMode: (diffMode) => {
      set({ diffMode });
      persistFromState(get);
    },
    markNotificationsSeen: (at) => {
      const stamp = at ?? new Date().toISOString();
      set({ notificationsLastSeen: stamp });
      persistFromState(get);
    },
    reset: () => {
      set(DEFAULTS);
      persist(DEFAULTS);
    },
  };
});

function persistFromState(get: () => PreferencesState): void {
  const s = get();
  persist({
    theme: s.theme,
    density: s.density,
    codeFontSize: s.codeFontSize,
    dateFormat: s.dateFormat,
    keyboardMode: s.keyboardMode,
    reposView: s.reposView,
    diffMode: s.diffMode,
    notificationsLastSeen: s.notificationsLastSeen,
  });
}
