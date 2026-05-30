// global.d.ts — ambient declarations for the SPA.
//
// React 19 removed the global `JSX` namespace; component return types must
// reference `React.JSX.Element` explicitly. We re-export the namespace into
// the global scope so existing `: JSX.Element` annotations continue to work
// without spelunking through every file.

import type { JSX as ReactJsx } from 'react';

declare global {
  namespace JSX {
    type Element = ReactJsx.Element;
    type ElementType = ReactJsx.ElementType;
    type ElementClass = ReactJsx.ElementClass;
    type IntrinsicElements = ReactJsx.IntrinsicElements;
  }
}

export {};
