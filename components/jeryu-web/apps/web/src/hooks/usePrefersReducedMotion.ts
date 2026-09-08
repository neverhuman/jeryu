// usePrefersReducedMotion.ts — reactive `prefers-reduced-motion` media query.
//
// Returns true when the user has asked the OS to minimize motion. Used to
// disable the boot splash animation and the feature-carousel auto-advance.

import { useEffect, useState } from 'react';

const QUERY = '(prefers-reduced-motion: reduce)';

function readInitial(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia(QUERY).matches
  );
}

export function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState<boolean>(readInitial);

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
      return;
    }
    const mq = window.matchMedia(QUERY);
    const onChange = (): void => setReduced(mq.matches);
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, []);

  return reduced;
}
