// MarkdownRenderer.test.tsx — XSS guard test (W-FE-09).
//
// The server already sanitizes with ammonia (§35.1.4); the client runs
// DOMPurify on the same HTML as a defense-in-depth layer. This test pins the
// contract: any `<script>` injected into the HTML is stripped before it
// reaches the DOM.

import { render } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';

import {
  MarkdownRenderer,
  sanitizeMarkdownHtml,
} from '../MarkdownRenderer';

describe('MarkdownRenderer', () => {
  it('strips <script> payloads from rendered HTML', () => {
    const xss = `<p>hello</p><script>alert(1)</script>`;
    const sanitized = sanitizeMarkdownHtml(xss);
    expect(sanitized).not.toContain('<script>');
    expect(sanitized).not.toContain('alert(1)');
    expect(sanitized).toContain('<p>hello</p>');
  });

  it('renders safe HTML inside a .markdown-body container', () => {
    const { container } = render(
      <MemoryRouter>
        <MarkdownRenderer html="<h1>Title</h1><script>alert(1)</script><p>body</p>" />
      </MemoryRouter>
    );
    const body = container.querySelector('.markdown-body');
    expect(body).not.toBeNull();
    expect(body?.querySelector('script')).toBeNull();
    expect(body?.querySelector('h1')?.textContent).toBe('Title');
    expect(body?.querySelector('p')?.textContent).toBe('body');
  });

  it('strips javascript: URLs from anchors', () => {
    const sanitized = sanitizeMarkdownHtml(
      `<a href="javascript:alert(1)">click</a>`
    );
    expect(sanitized).not.toMatch(/javascript:/i);
  });
});
