// KeyboardShortcutsOverlay.stories.tsx — registry rendering states (W-CC-04).
//
// Mounts the provider with SEVERAL components registering shortcuts at once —
// the exact multi-consumer registry shape that previously re-render-looped —
// and opens the overlay via its own `shift+/` binding so the grouped listing
// is reviewable. A registry that fails to settle would hang/crash this story
// instead of rendering.

import type { Meta, StoryObj } from '@storybook/react-vite';

import {
  KeyboardProvider,
  useKeyboardShortcut,
} from '../hooks/useKeyboard';

import { KeyboardShortcutsOverlay } from './KeyboardShortcutsOverlay';

function RegisteredShortcut({
  combo,
  label,
  group,
}: {
  combo: string;
  label: string;
  group: string;
}): JSX.Element {
  useKeyboardShortcut(combo, () => {}, { label, group });
  return <></>;
}

function OverlayHarness(): JSX.Element {
  return (
    <KeyboardProvider>
      <RegisteredShortcut combo="g r" label="Go to repositories" group="Navigation" />
      <RegisteredShortcut combo="g d" label="Go to dashboard" group="Navigation" />
      <RegisteredShortcut combo="Mod+K" label="Open command palette" group="Commands" />
      <KeyboardShortcutsOverlay />
      <p>
        Press <kbd>?</kbd> to open the shortcut help overlay.
      </p>
    </KeyboardProvider>
  );
}

const meta: Meta<typeof OverlayHarness> = {
  title: 'foundation/KeyboardShortcutsOverlay',
  component: OverlayHarness,
};
export default meta;

type Story = StoryObj<typeof OverlayHarness>;

export const RegistrySettlesWithMultipleConsumers: Story = {};

export const OpenedViaShortcut: Story = {
  play: async ({ canvasElement }) => {
    canvasElement.ownerDocument.defaultView?.dispatchEvent(
      new KeyboardEvent('keydown', { key: '?', shiftKey: true })
    );
  },
};
