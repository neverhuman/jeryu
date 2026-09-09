// JeryuLogo.tsx — the JeRyu wordmark + terminal-prompt glyph (TUI overhaul).
//
// One component, three lockups:
//   * `header` — compact glyph + "JeRyu" wordmark for the global header.
//   * `mark`   — the glyph alone (login panel, tight spots).
//   * `hero`   — oversized, glowing wordmark inside a box-drawing frame for
//                the splash / boot screen.
//
// Casing is canonical **JeRyu** everywhere. Colors come from tokens; the neon
// glow is a CSS `filter`/`text-shadow` so it never lowers text contrast.

import './JeryuLogo.css';

export type JeryuLogoVariant = 'header' | 'mark' | 'hero';

export interface JeryuLogoProps {
  variant?: JeryuLogoVariant;
  /** Accessible label; defaults to the product name. */
  title?: string;
  /** Render purely decorative (aria-hidden, no role/label) — use when an
   *  adjacent heading already names the brand for assistive tech. */
  decorative?: boolean;
  className?: string;
}

/** The terminal-prompt glyph: a square "window" enclosing a `>` prompt. */
function PromptGlyph(): JSX.Element {
  return (
    <svg
      className="jeryu-logo__glyph"
      viewBox="0 0 24 24"
      width="1em"
      height="1em"
      fill="none"
      aria-hidden="true"
      focusable="false"
    >
      <rect
        x="1.5"
        y="1.5"
        width="21"
        height="21"
        stroke="currentColor"
        strokeWidth="1.5"
      />
      <path
        d="M6.5 8.5L10.5 12L6.5 15.5"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="square"
        strokeLinejoin="miter"
      />
      <path
        d="M12.5 15.5H17.5"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="square"
      />
    </svg>
  );
}

/** The styled wordmark. `Je` and `yu` are body ink; `R` glows cyan. */
function Wordmark(): JSX.Element {
  return (
    <span className="jeryu-logo__wordmark" aria-hidden="true">
      Je<span className="jeryu-logo__wordmark-accent">R</span>yu
    </span>
  );
}

export function JeryuLogo({
  variant = 'header',
  title = 'JeRyu',
  decorative = false,
  className,
}: JeryuLogoProps): JSX.Element {
  const classes = `jeryu-logo jeryu-logo--${variant}${
    className ? ` ${className}` : ''
  }`;
  const a11y = decorative
    ? ({ 'aria-hidden': true } as const)
    : ({ role: 'img', 'aria-label': title } as const);

  if (variant === 'mark') {
    return (
      <span className={classes} {...a11y}>
        <PromptGlyph />
      </span>
    );
  }

  if (variant === 'hero') {
    return (
      <span className={classes} {...a11y}>
        <span className="jeryu-logo__frame" aria-hidden="true">
          <span className="jeryu-logo__frame-top">
            ┌─ JeRyu ──────────────┐
          </span>
          <span className="jeryu-logo__hero-row">
            <PromptGlyph />
            <Wordmark />
          </span>
          <span className="jeryu-logo__frame-bottom">
            └──────────── forge ───┘
          </span>
        </span>
      </span>
    );
  }

  // header (default)
  return (
    <span className={classes} {...a11y}>
      <PromptGlyph />
      <Wordmark />
    </span>
  );
}
