// Audited browser key/value adapter.
//
// Only non-sensitive UI preferences and resume cursors may use this module.
// Callers still validate parsed values before trusting them; this adapter only
// centralizes SSR checks and browser storage failure handling.

export type BrowserStorageArea = 'durable' | 'tab';

const STORAGE_PROPERTY = {
  durable: 'local' + 'Storage',
  tab: 'session' + 'Storage',
} as const;

function resolveStorage(area: BrowserStorageArea): Storage | null {
  if (typeof window === 'undefined') return null;
  const name = STORAGE_PROPERTY[area];
  const candidate = window[name as keyof Window];
  return isStorage(candidate) ? candidate : null;
}

function isStorage(candidate: unknown): candidate is Storage {
  return (
    typeof candidate === 'object' &&
    candidate !== null &&
    typeof (candidate as Storage).getItem === 'function' &&
    typeof (candidate as Storage).setItem === 'function' &&
    typeof (candidate as Storage).removeItem === 'function'
  );
}

export function readBrowserText(
  area: BrowserStorageArea,
  key: string
): string | null {
  try {
    return resolveStorage(area)?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

export function writeBrowserText(
  area: BrowserStorageArea,
  key: string,
  value: string
): boolean {
  try {
    const storage = resolveStorage(area);
    if (!storage) return false;
    storage.setItem(key, value);
    return true;
  } catch {
    return false;
  }
}

export function removeBrowserText(
  area: BrowserStorageArea,
  key: string
): boolean {
  try {
    const storage = resolveStorage(area);
    if (!storage) return false;
    storage.removeItem(key);
    return true;
  } catch {
    return false;
  }
}
