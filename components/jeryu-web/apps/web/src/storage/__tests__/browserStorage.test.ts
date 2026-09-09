import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  readBrowserText,
  removeBrowserText,
  writeBrowserText,
} from '../browserStorage';

function makeStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => Array.from(values.keys())[index] ?? null,
    removeItem: (key) => {
      values.delete(key);
    },
    setItem: (key, value) => {
      values.set(key, value);
    },
  };
}

describe('browser storage adapter', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('round-trips durable preference text', () => {
    Object.defineProperty(window, 'localStorage', {
      configurable: true,
      value: makeStorage(),
    });

    expect(writeBrowserText('durable', 'jeryu.test.preference', 'compact')).toBe(
      true
    );
    expect(readBrowserText('durable', 'jeryu.test.preference')).toBe('compact');
  });

  it('round-trips tab-scoped cursor text and removes it', () => {
    Object.defineProperty(window, 'sessionStorage', {
      configurable: true,
      value: makeStorage(),
    });

    expect(writeBrowserText('tab', 'jeryu.test.cursor', '42')).toBe(true);
    expect(readBrowserText('tab', 'jeryu.test.cursor')).toBe('42');
    expect(removeBrowserText('tab', 'jeryu.test.cursor')).toBe(true);
    expect(readBrowserText('tab', 'jeryu.test.cursor')).toBeNull();
  });

  it('treats disabled browser storage as unavailable', () => {
    Object.defineProperty(window, 'localStorage', {
      configurable: true,
      value: {
        getItem: () => {
          throw new DOMException('disabled', 'SecurityError');
        },
        setItem: () => {
          throw new DOMException('quota', 'QuotaExceededError');
        },
        removeItem: () => {
          throw new DOMException('disabled', 'SecurityError');
        },
      },
    });

    expect(writeBrowserText('durable', 'jeryu.test.preference', 'table')).toBe(
      false
    );
    expect(readBrowserText('durable', 'jeryu.test.preference')).toBeNull();
  });
});
