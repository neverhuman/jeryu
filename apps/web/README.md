# `@jeryu/web`

Vite + React + TypeScript SPA for the JeRyu Web Forge.

## Pointers

- Plan: [`/home/ubuntu/jeryu/WEB_WORK_CLAUDE.md`](../../WEB_WORK_CLAUDE.md)
- Architecture: `docs/web-forge.md` (placeholder — produced by W-D-01)
- Frontend guide: this file plus `docs/web-forge.md` (W-D-06)

## Scripts (from repo root)

```sh
npm run dev                # vite dev server on http://127.0.0.1:5173
npm run build              # tsc -b && vite build → apps/web/dist/
npm run preview            # vite preview on http://127.0.0.1:4173
npm run typecheck          # tsc -b --pretty false
npm run lint               # eslint .
npm run test               # vitest run
npm run test:e2e           # playwright test (see playwright.config.ts)
npm run storybook          # storybook dev on http://127.0.0.1:6006
npm run build-storybook    # storybook build → storybook-static/
```

## Status

This is the skeleton from W-F-07 / W-F-09 / W-F-12. Tokens, layout,
routing, API client, and feature pages will be filled in by W-FE-* per
WEB_WORK_CLAUDE.md §7.4.
