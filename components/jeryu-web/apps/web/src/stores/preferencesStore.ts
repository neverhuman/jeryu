// preferencesStore.ts — user UI preferences (W-FE-05 + W-CC-01).
//
// State persists through the audited browser storage adapter as
// `jeryu.preferences.v1`. The version suffix lets us migrate the schema later
// without poisoning user data.

import { create } from 'zustand';

import { readBrowserText, writeBrowserText } from '../storage/browserStorage';

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

// v2: the TUI overhaul flips the default theme to the dark terminal surface.
// Bumping the version resets stored prefs so returning users land on the new
// default instead of a stale `system` (which could resolve to light).
const STORAGE_KEY = 'jeryu.preferences.v2';

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
  theme: 'dark',
  density: 'comfortable',
  codeFontSize: 13,
  dateFormat: 'relative',
  keyboardMode: 'default',
  reposView: 'card',
  diffMode: 'unified',
  notificationsLastSeen: null,
};

function loadInitial(): typeof DEFAULTS {
  try {
    const raw = readBrowserText('durable', STORAGE_KEY);
    if (!raw) return DEFAULTS;
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
  writeBrowserText('durable', STORAGE_KEY, JSON.stringify(state));
}

function validateTheme(input: unknown): ThemePreference | null {
  return input === 'system' ||
    input === 'light' ||
    input === 'dark' ||
    input === 'high-contrast'
    ? input
    : null;
}

function validateDensity(input: unknown): DensityPreference | undefined {
  return input === 'comfortable' ||
    input === 'compact' ||
    input === 'ultra-compact'
    ? input
    : undefined;
}

function validateDateFormat(input: unknown): DateFormat | undefined {
  return input === 'relative' || input === 'iso' || input === 'long'
    ? input
    : undefined;
}

function validateKeyboardMode(input: unknown): KeyboardMode | undefined {
  return input === 'default' || input === 'vim' ? input : undefined;
}

function validateReposView(input: unknown): ReposViewMode | undefined {
  return input === 'card' || input === 'table' ? input : undefined;
}

function validateDiffMode(input: unknown): DiffMode | undefined {
  return input === 'unified' || input === 'split' ? input : undefined;
}

function validateIsoTimestamp(input: unknown): string | undefined {
  if (typeof input !== 'string') return;
  const ms = Date.parse(input);
  return Number.isFinite(ms) ? input : undefined;
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
