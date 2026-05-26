import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import './styles/tokens.css';
import './styles/app.css';

const rootEl = document.getElementById('root');
if (!rootEl) {
  throw new Error('JeRyu Web Forge: #root element not found in index.html');
}

createRoot(rootEl).render(
  <StrictMode>
    <div id="root">Hello JeRyu Web Forge</div>
  </StrictMode>
);
