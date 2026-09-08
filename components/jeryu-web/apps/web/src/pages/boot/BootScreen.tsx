// BootScreen.tsx — signed-out splash → story landing with optional auth panel.
//
// Rendered by AppShell in place of the app whenever there is no authenticated
// user. `/` shows the story first; `/login` and `/signup` open the shared auth
// form immediately over the same surface.

import { useEffect, useRef, useState } from 'react';

import { JeryuLogo } from '../../components/brand/JeryuLogo';
import { usePrefersReducedMotion } from '../../hooks/usePrefersReducedMotion';
import { BootSplash } from './BootSplash';
import { LoginPanel } from './LoginPanel';

import './boot.css';

type Mode = 'login' | 'signup';
const SPLASH_MS = 950;

export function BootScreen({
  initialMode = 'login',
  initialAuthOpen = false,
}: {
  initialMode?: Mode;
  initialAuthOpen?: boolean;
}): JSX.Element {
  const prefersReduced = usePrefersReducedMotion();
  const [phase, setPhase] = useState<'splash' | 'gate'>(
    prefersReduced || initialAuthOpen ? 'gate' : 'splash'
  );
  const [mode, setMode] = useState<Mode>(initialMode);
  const [authOpen, setAuthOpen] = useState(initialAuthOpen);
  const usernameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setMode(initialMode);
    setAuthOpen(initialAuthOpen);
    if (initialAuthOpen) {
      setPhase('gate');
    }
  }, [initialAuthOpen, initialMode]);

  // Advance past the splash on a timer, or immediately on any key / pointer.
  useEffect(() => {
    if (phase !== 'splash') return;
    const toGate = (): void => setPhase('gate');
    const timer = window.setTimeout(toGate, SPLASH_MS);
    window.addEventListener('keydown', toGate, { once: true });
    window.addEventListener('pointerdown', toGate, { once: true });
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener('keydown', toGate);
      window.removeEventListener('pointerdown', toGate);
    };
  }, [phase]);

  useEffect(() => {
    if (phase === 'gate' && authOpen) {
      usernameRef.current?.focus();
    }
  }, [authOpen, mode, phase]);

  const openAuth = (nextMode: Mode): void => {
    setMode(nextMode);
    setAuthOpen(true);
  };

  if (phase === 'splash') {
    return (
      <main className="boot boot--splash" aria-label="Starting JeRyu">
        <BootSplash />
        <p className="sr-only">Starting JeRyu…</p>
      </main>
    );
  }

  return (
    <main className={`boot boot--gate${authOpen ? ' boot--auth-open' : ''}`}>
      <div className="boot__story">
        <header className="boot__nav">
          <span className="boot__brand">
            <JeryuLogo variant="mark" className="boot__brand-mark" decorative />
            <span>JeRyu</span>
          </span>
          <nav className="boot__auth-controls" aria-label="Account access">
            <button
              type="button"
              className="boot__auth-link"
              aria-pressed={authOpen && mode === 'login'}
              onClick={() => openAuth('login')}
            >
              Log in
            </button>
            <button
              type="button"
              className="boot__auth-link"
              aria-pressed={authOpen && mode === 'signup'}
              onClick={() => openAuth('signup')}
            >
              Sign up
            </button>
          </nav>
        </header>

        <section className="boot__hero" aria-label="JeRyu product story">
          <div className="boot__hero-copy">
            <p className="boot__eyebrow">Rust forge core · React cockpit</p>
            <h1>Git for agents.</h1>
            <p className="boot__lede">
              Made by agents, for agents: a Rust forge core with a React cockpit
              for repo families moving at machine speed.
            </p>
            <p className="boot__story-rail">
              Issue -&gt; agent session -&gt; evidence -&gt; PR -&gt; gated merge -&gt; autonomous deploy
            </p>
            <div className="boot__gains" aria-label="JeRyu gains">
              <p>Preserve agent work.</p>
              <p>Remove release handoffs.</p>
              <p>Operate billion-token/day repo families at 1000-PR/day pace.</p>
            </div>
          </div>
        </section>

        {authOpen ? (
          <aside className="boot__auth-panel">
            <LoginPanel initialMode={mode} firstFieldRef={usernameRef} />
          </aside>
        ) : null}
      </div>
    </main>
  );
}
