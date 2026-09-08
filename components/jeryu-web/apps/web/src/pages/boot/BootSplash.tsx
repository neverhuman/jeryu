// BootSplash.tsx — the brief "cold boot" animation before the login gate.
//
// Purely decorative: an oversized glowing JeRyu wordmark, a staggered boot
// log, and a progress bar. Timing + skip handling live in BootScreen; the
// visuals here are CSS-driven (keyframes + animation-delay) so they stay
// smooth. Rendered only when motion is allowed.

import { JeryuLogo } from '../../components/brand/JeryuLogo';

const BOOT_LOG = [
  '› linking control-plane…',
  '› mounting forge…',
  '› syncing realtime bus…',
  '› ready.',
];

export function BootSplash(): JSX.Element {
  return (
    <div className="boot-splash" aria-hidden="true">
      <JeryuLogo variant="hero" decorative />
      <ul className="boot-splash__log">
        {BOOT_LOG.map((line, i) => (
          <li
            key={line}
            className="boot-splash__log-line"
            style={{ animationDelay: `${120 + i * 160}ms` }}
          >
            {line}
          </li>
        ))}
      </ul>
      <div className="boot-splash__bar">
        <span className="boot-splash__bar-fill" />
      </div>
    </div>
  );
}
