// ReadmePanel.stories.tsx — README rendering surface (W-T-07).
//
// `ReadmePanel` consumes the `useMarkdown` query hook directly so we can't
// drive its states purely with props. Instead, the "loading / empty /
// rendered / malicious-HTML-sanitized" archetypes wrap `MarkdownRenderer`
// (the inner surface) plus the shared `EmptyState` / `LoadingState`
// components so the addon-a11y panel still scans the visual chrome each
// state lights up.

import type { Meta, StoryObj } from '@storybook/react-vite';
import { FileText } from 'lucide-react';

import { MarkdownRenderer } from './MarkdownRenderer';
import { EmptyState, LoadingState } from '../state';

const meta: Meta<typeof MarkdownRenderer> = {
  title: 'browser/ReadmePanel',
  component: MarkdownRenderer,
};
export default meta;

type Story = StoryObj<typeof MarkdownRenderer>;

const SAFE_HTML = `
<h1 id="jeryu">JeRyu</h1>
<p>Welcome to the <strong>JeRyu Web Forge</strong> README. This document is
rendered through the <code>ammonia</code> + <code>DOMPurify</code> double
sanitizer pipeline.</p>
<h2 id="getting-started">Getting started</h2>
<ol>
  <li>Clone the repo.</li>
  <li>Run <code>just fast</code>.</li>
  <li>Open <a href="/repos/jeryu/veox/redline">veox/redline</a>.</li>
</ol>
<pre><code class="language-rust">fn main() {
    println!("hello, world");
}</code></pre>
`;

// Intentionally evil — DOMPurify must strip the script and event handlers.
const MALICIOUS_HTML = `
<h1>Spec README</h1>
<p>Hello.</p>
<script>alert('xss')</script>
<img src="x" onerror="alert('xss')" />
<a href="javascript:alert('xss')">click</a>
<iframe src="https://attacker.example/iframe"></iframe>
<p>Footer below the strip.</p>
`;

export const Loading: Story = {
  render: () => (
    <LoadingState
      title="Loading README…"
      description="Fetching rendered Markdown."
      rows={6}
    />
  ),
};

export const Empty: Story = {
  render: () => (
    <EmptyState
      icon={FileText}
      title="No README found"
      description="This repository does not include a README at the selected ref."
    />
  ),
};

export const Rendered: Story = {
  args: {
    html: SAFE_HTML,
  },
};

export const MaliciousHtmlSanitized: Story = {
  name: 'Malicious HTML sanitized',
  args: {
    html: MALICIOUS_HTML,
  },
};
