// MarkdownRenderer.test.tsx — XSS guard test (W-FE-09).
//
// The server already sanitizes with ammonia (§35.1.4); the client runs
// DOMPurify on the same HTML as a defense-in-depth layer. This test pins the
// contract: any `<script>` injected into the HTML is stripped before it
// reaches the DOM.

import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { describe, expect, it } from 'vitest';

import {
  MarkdownRenderer,
  sanitizeMarkdownHtml,
} from '../MarkdownRenderer';

describe('MarkdownRenderer', () => {
  function LocationProbe(): JSX.Element {
    const location = useLocation();
    return <div data-testid="location">{location.pathname}</div>;
  }

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

  it('keeps internal links local and hardens external links on click', () => {
    const { container } = render(
      <MemoryRouter initialEntries={['/repos/jeryu/root']}>
        <LocationProbe />
        <MarkdownRenderer
          html="<p><a href='/repos/jeryu/demo'>Repo</a> <a href='https://example.com'>External</a></p>"
        />
      </MemoryRouter>
    );

    const links = container.querySelectorAll('a');
    expect(links).toHaveLength(2);

    const [internalLink, externalLink] = Array.from(links);
    expect(internalLink.getAttribute('href')).toBe('/repos/jeryu/demo');
    expect(externalLink.getAttribute('href')).toBe('https://example.com');
    expect(externalLink.getAttribute('target')).toBe('_blank');
    expect(externalLink.getAttribute('rel')).toBe('noopener noreferrer');

    fireEvent.click(externalLink);

    fireEvent.click(internalLink);
    expect(screen.getByTestId('location')).toHaveTextContent(
      '/repos/jeryu/demo'
    );
  });
});
