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

export interface PreferencesState {
  theme: ThemePreference;
  density: DensityPreference;
  codeFontSize: number;
  dateFormat: DateFormat;
  keyboardMode: KeyboardMode;
  reposView: ReposViewMode;
  setTheme: (theme: ThemePreference) => void;
  setDensity: (density: DensityPreference) => void;
  setCodeFontSize: (size: number) => void;
  setDateFormat: (format: DateFormat) => void;
  setKeyboardMode: (mode: KeyboardMode) => void;
  setReposView: (view: ReposViewMode) => void;
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
> = {
  theme: 'system',
  density: 'comfortable',
  codeFontSize: 13,
  dateFormat: 'relative',
  keyboardMode: 'default',
  reposView: 'card',
};

function loadInitial(): typeof DEFAULTS {
  if (typeof window === 'undefined') return DEFAULTS;
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<typeof DEFAULTS>;
    return {
      theme: validateTheme(parsed.theme) ?? DEFAULTS.theme,
      density: validateDensity(parsed.density) ?? DEFAULTS.density,
      codeFontSize:
        typeof parsed.codeFontSize === 'number' &&
        parsed.codeFontSize >= 10 &&
        parsed.codeFontSize <= 24
          ? parsed.codeFontSize
          : DEFAULTS.codeFontSize,
      dateFormat: validateDateFormat(parsed.dateFormat) ?? DEFAULTS.dateFormat,
      keyboardMode:
        validateKeyboardMode(parsed.keyboardMode) ?? DEFAULTS.keyboardMode,
      reposView: validateReposView(parsed.reposView) ?? DEFAULTS.reposView,
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
  });
}
