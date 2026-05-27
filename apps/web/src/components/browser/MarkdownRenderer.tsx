// MarkdownRenderer.tsx — sanitized HTML renderer (W-FE-09).
//
// The server sanitizes Markdown through ammonia per §35.1.4 before returning
// HTML; we re-sanitize with DOMPurify in the browser as defense-in-depth and
// drop the result into a `.markdown-body` container so the global stylesheet
// styles it. Anchor clicks are intercepted so internal `/repos/...` links
// stay inside the SPA router and external links open in a new tab with safe
// `rel` attributes.

import DOMPurify, { type Config as DOMPurifyConfig } from 'dompurify';
import { useEffect, useMemo, useRef } from 'react';
import { useNavigate } from 'react-router-dom';

import './browser.css';

export interface MarkdownRendererProps {
  /** Sanitized HTML (the server already ran ammonia). */
  html: string;
  /** Extra classes appended to `.markdown-body`. */
  className?: string;
}

/** Sanitization config — we strip every script-bearing attribute defensively. */
const PURIFY_CONFIG: DOMPurifyConfig = {
  USE_PROFILES: { html: true },
  // Drop URLs that aren't on the http(s)/mailto/relative allowlist; the
  // server already enforces this but a second layer is cheap.
  ALLOWED_URI_REGEXP:
    /^(?:(?:https?|mailto|tel|sms):|[^a-z]|[a-z+.-]+(?:[^a-z+.\-:]|$))/i,
  ADD_ATTR: ['target', 'rel'],
};

export function sanitizeMarkdownHtml(html: string): string {
  return DOMPurify.sanitize(html, PURIFY_CONFIG) as unknown as string;
}

export function MarkdownRenderer({
  html,
  className,
}: MarkdownRendererProps): JSX.Element {
  const navigate = useNavigate();
  const containerRef = useRef<HTMLDivElement | null>(null);

  const safeHtml = useMemo(() => sanitizeMarkdownHtml(html), [html]);

  useEffect(() => {
    const root = containerRef.current;
    if (!root) return undefined;
    const onClick = (event: MouseEvent): void => {
      // Only intercept plain left-clicks; let modifiers (open-in-new-tab) and
      // middle-clicks fall through to the browser's default handling.
      if (
        event.defaultPrevented ||
        event.button !== 0 ||
        event.metaKey ||
        event.ctrlKey ||
        event.shiftKey ||
        event.altKey
      ) {
        return;
      }
      const target = event.target as Element | null;
      if (!target) return;
      const anchor = target.closest<HTMLAnchorElement>('a[href]');
      if (!anchor || !root.contains(anchor)) return;
      const href = anchor.getAttribute('href') ?? '';
      if (href.startsWith('#')) {
        // Anchor links — let the browser scroll. The sanitized HTML keeps
        // heading ids so in-doc navigation works.
        return;
      }
      if (href.startsWith('/repos/') || href.startsWith('/')) {
        event.preventDefault();
        navigate(href);
        return;
      }
      // External link — open in a new tab and harden the rel.
      if (/^https?:\/\//.test(href)) {
        if (anchor.target !== '_blank') anchor.target = '_blank';
        const rel = anchor.getAttribute('rel') ?? '';
        if (!/noopener/.test(rel) || !/noreferrer/.test(rel)) {
          anchor.setAttribute(
            'rel',
            `${rel} noopener noreferrer`.trim()
          );
        }
      }
    };
    root.addEventListener('click', onClick);
    return () => root.removeEventListener('click', onClick);
  }, [navigate, safeHtml]);

  return (
    <div
      ref={containerRef}
      className={`markdown-body ${className ?? ''}`.trim()}
      // Server output is already sanitized by ammonia (§35.1.4); the
      // DOMPurify pass above adds defense-in-depth.
      // eslint-disable-next-line react/no-danger
      dangerouslySetInnerHTML={{ __html: safeHtml }}
    />
  );
}
