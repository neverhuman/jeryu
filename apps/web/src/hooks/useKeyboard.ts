// useKeyboard.ts — keyboard shortcut foundation (W-CC-04).
//
// Two primitives:
//   * `useKeyboardShortcut(combo, handler, opts)` — register a single shortcut
//     for the lifetime of the calling component. Supports modifier combos
//     ("Mod+K", "Shift+/") and chord sequences ("g r").
//   * `KeyboardContext` — exposes a registry so components can introspect
//     the active shortcuts (the help overlay reads this).
//
// `Mod` resolves to ⌘ on macOS and Ctrl elsewhere.

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  createElement,
} from 'react';

export interface KeyboardShortcutDescriptor {
  combo: string;
  label: string;
  group?: string;
}

interface KeyboardContextValue {
  shortcuts: KeyboardShortcutDescriptor[];
  registerShortcut: (descriptor: KeyboardShortcutDescriptor) => () => void;
}

const KeyboardContext = createContext<KeyboardContextValue | null>(null);

export function KeyboardProvider({
  children,
}: {
  children: ReactNode;
}): JSX.Element {
  const [shortcuts, setShortcuts] = useState<KeyboardShortcutDescriptor[]>(
    []
  );

  const registerShortcut = useCallback(
    (descriptor: KeyboardShortcutDescriptor) => {
      setShortcuts((prev) => {
        if (prev.some((s) => s.combo === descriptor.combo)) return prev;
        return [...prev, descriptor];
      });
      return () => {
        setShortcuts((prev) =>
          prev.filter((s) => s.combo !== descriptor.combo)
        );
      };
    },
    []
  );

  const value = useMemo<KeyboardContextValue>(
    () => ({ shortcuts, registerShortcut }),
    [shortcuts, registerShortcut]
  );

  return createElement(KeyboardContext.Provider, { value }, children);
}

export function useKeyboardRegistry(): KeyboardContextValue {
  const ctx = useContext(KeyboardContext);
  if (!ctx) {
    throw new Error(
      'useKeyboardRegistry must be used inside <KeyboardProvider>.'
    );
  }
  return ctx;
}

interface ShortcutOptions {
  /** Human label for the help overlay. */
  label?: string;
  /** Group key for the help overlay (e.g. "Navigation", "Editing"). */
  group?: string;
  /** When true, shortcuts trigger even inside input/textarea/contentEditable. */
  allowInInputs?: boolean;
  /** Pause the shortcut without unmounting the component. */
  enabled?: boolean;
  /** Override the registry label/visibility — pass false to keep it hidden. */
  registerInHelp?: boolean;
}

const isMacLike = (): boolean => {
  if (typeof navigator === 'undefined') return false;
  return /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
};

interface ParsedAtom {
  key: string;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
}

function parseAtom(atom: string): ParsedAtom {
  const parts = atom.split('+').map((p) => p.trim());
  const out: ParsedAtom = {
    key: '',
    ctrl: false,
    alt: false,
    shift: false,
    meta: false,
  };
  const mac = isMacLike();
  for (const part of parts) {
    const lower = part.toLowerCase();
    if (lower === 'mod') {
      if (mac) out.meta = true;
      else out.ctrl = true;
    } else if (lower === 'cmd' || lower === 'meta') out.meta = true;
    else if (lower === 'ctrl' || lower === 'control') out.ctrl = true;
    else if (lower === 'alt' || lower === 'option') out.alt = true;
    else if (lower === 'shift') out.shift = true;
    else out.key = normalizeKey(lower);
  }
  return out;
}

function normalizeKey(key: string): string {
  if (key === 'space') return ' ';
  if (key === 'esc') return 'escape';
  if (key === 'return') return 'enter';
  return key;
}

function matchesAtom(atom: ParsedAtom, event: KeyboardEvent): boolean {
  if (atom.ctrl !== event.ctrlKey) return false;
  if (atom.alt !== event.altKey) return false;
  if (atom.meta !== event.metaKey) return false;
  // Shift is special: when atom.shift is false but the user must press Shift
  // for the printable character (e.g. "?"), we let the key compare succeed.
  if (atom.shift && !event.shiftKey) return false;
  const eventKey = (event.key || '').toLowerCase();
  return eventKey === atom.key;
}

function isTypingInTextField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  if (target.isContentEditable) return true;
  return false;
}

const CHORD_TIMEOUT_MS = 1200;

export function useKeyboardShortcut(
  combo: string,
  handler: (event: KeyboardEvent) => void,
  options: ShortcutOptions = {}
): void {
  const ctx = useContext(KeyboardContext);
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  const atoms = useMemo(() => {
    return combo
      .trim()
      .split(/\s+/)
      .filter(Boolean)
      .map(parseAtom);
  }, [combo]);

  useEffect(() => {
    if (options.registerInHelp === false || !ctx) return undefined;
    return ctx.registerShortcut({
      combo,
      label: options.label ?? combo,
      group: options.group,
    });
  }, [combo, ctx, options.label, options.group, options.registerInHelp]);

  useEffect(() => {
    if (options.enabled === false || atoms.length === 0) return undefined;
    let chordIndex = 0;
    let chordTimer: ReturnType<typeof setTimeout> | null = null;

    const onKey = (event: KeyboardEvent): void => {
      if (!options.allowInInputs && isTypingInTextField(event.target)) {
        return;
      }
      const expected = atoms[chordIndex];
      if (!expected || !matchesAtom(expected, event)) {
        chordIndex = 0;
        if (chordTimer) clearTimeout(chordTimer);
        chordTimer = null;
        return;
      }
      event.preventDefault();
      chordIndex += 1;
      if (chordIndex >= atoms.length) {
        chordIndex = 0;
        if (chordTimer) clearTimeout(chordTimer);
        chordTimer = null;
        handlerRef.current(event);
        return;
      }
      if (chordTimer) clearTimeout(chordTimer);
      chordTimer = setTimeout(() => {
        chordIndex = 0;
      }, CHORD_TIMEOUT_MS);
    };

    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
      if (chordTimer) clearTimeout(chordTimer);
    };
  }, [atoms, options.enabled, options.allowInInputs]);
}
