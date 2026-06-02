// MarkdownRenderer.tsx — sanitized HTML renderer (W-FE-09).
//
// The server sanitizes Markdown through ammonia per §35.1.4 before returning
// HTML; we re-sanitize with DOMPurify in the browser as defense-in-depth and
// render the result into a `.markdown-body` container so the global stylesheet
// styles it. Internal `/repos/...` links stay inside the SPA router and
// external links are rendered with safe new-tab attributes.

import DOMPurify, { type Config as DOMPurifyConfig } from 'dompurify';
import { createElement, useMemo, type MouseEvent, type ReactNode } from 'react';
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
  ADD_ATTR: ['target', 'rel'],
};

function isSafeHref(href: string): boolean {
  return /^(?:(?:https?|mailto|tel|sms):|[^a-z]|[a-z+.-]+(?:[^a-z+.\-:]|$))/i.test(
    href
  );
}

export function sanitizeMarkdownHtml(html: string): string {
  // `PURIFY_CONFIG` sets neither `RETURN_DOM` nor `RETURN_DOM_FRAGMENT`, so
  // DOMPurify returns a string. Its overloaded signature still widens the
  // return to a DOM union; narrow it with a runtime check instead of an
  // unchecked cast so the proven `string` shape is what we hand to React.
  const result: unknown = DOMPurify.sanitize(html, PURIFY_CONFIG);
  return typeof result === 'string' ? result : '';
}

function attributeNameToProp(name: string): string {
  switch (name) {
    case 'class':
      return 'className';
    case 'for':
      return 'htmlFor';
    case 'tabindex':
      return 'tabIndex';
    case 'colspan':
      return 'colSpan';
    case 'rowspan':
      return 'rowSpan';
    case 'datetime':
      return 'dateTime';
    case 'maxlength':
      return 'maxLength';
    case 'minlength':
      return 'minLength';
    case 'readonly':
      return 'readOnly';
    default:
      return name;
  }
}

function isPlainLeftClick(event: MouseEvent): boolean {
  return (
    !event.defaultPrevented &&
    event.button === 0 &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.shiftKey &&
    !event.altKey
  );
}

function renderNode(
  node: ChildNode,
  key: string,
  onInternalLinkClick?: (href: string, event: MouseEvent) => void
): ReactNode {
  if (node.nodeType === Node.TEXT_NODE) {
    return node.textContent ?? '';
  }
  if (node.nodeType !== Node.ELEMENT_NODE) {
    return null;
  }

  const element = node as HTMLElement;
  const props: Record<string, unknown> & { key: string } = { key };
  const tagName = element.tagName.toLowerCase();
  for (const name of element.getAttributeNames()) {
    const value = element.getAttribute(name);
    if (value == null) continue;
    if (tagName === 'a' && name === 'href' && !isSafeHref(value)) {
      continue;
    }
    props[attributeNameToProp(name)] = value;
  }

  if (tagName === 'a') {
    const href = element.getAttribute('href') ?? '';
    if (href.startsWith('/repos/') || href.startsWith('/')) {
      props.onClick = (event: MouseEvent) => {
        if (!isPlainLeftClick(event)) {
          return;
        }
        event.preventDefault();
        onInternalLinkClick?.(href, event);
      };
    } else if (/^https?:\/\//.test(href)) {
      props.target = '_blank';
      props.rel = 'noopener noreferrer';
    }
  }

  const children = Array.from(element.childNodes).map((child, index) =>
    renderNode(child, `${key}.${index}`, onInternalLinkClick)
  );
  return createElement(tagName, props as never, ...children);
}

function renderSanitizedHtml(
  html: string,
  onInternalLinkClick?: (href: string, event: MouseEvent) => void
): ReactNode {
  const parser = new DOMParser();
  const document = parser.parseFromString(html, 'text/html');
  return Array.from(document.body.childNodes).map((child, index) =>
    renderNode(child, `markdown.${index}`, onInternalLinkClick)
  );
}

export function MarkdownRenderer({
  html,
  className,
}: MarkdownRendererProps): JSX.Element {
  const navigate = useNavigate();

  // Memoize the sanitized, typed React tree so we parse once per input rather
  // than on every render.
  const renderedContent = useMemo(
    () =>
      renderSanitizedHtml(sanitizeMarkdownHtml(html), (href) => {
        navigate(href);
      }),
    [html, navigate]
  );

  return (
    <div
      className={`markdown-body ${className ?? ''}`.trim()}
      // The server already sanitized the Markdown through ammonia (§35.1.4);
      // the browser reruns DOMPurify and then renders the sanitized tree with
      // typed React elements instead of a raw HTML sink.
    >
      {renderedContent}
    </div>
  );
}
