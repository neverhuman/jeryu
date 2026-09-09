// main.tsx — JeRyu Web Forge entry point (W-FE-02).
//
// Mounts <App /> into #root with React StrictMode. Stylesheets are imported
// here so Vite ships them in the initial bundle.

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import './styles/fonts.css';
import './styles/tokens.css';
import './styles/app.css';

import { App } from './app/App';

const rootEl = document.getElementById('root');
if (!rootEl) {
  throw new Error('JeRyu Web Forge: #root element not found in index.html');
}

createRoot(rootEl).render(
  <StrictMode>
    <App />
  </StrictMode>
);
