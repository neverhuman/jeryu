// fleetModel.ts — pure selectors that fold the live WS event buffer + the
// bootstrap `tui` snapshot into a normalized, render-ready fleet view.
//
// The /fleet page surfaces the server-wide runner-pool fabric across ALL
// repos: per-pool utilization, queued/running/failed jobs, online/stuck
// runners, saturation/pause state, the system-health strip, and the derived
// bottleneck banner. Two data sources feed it and both speak the same Rust
// read-model contract (`jeryu_readmodel::pool_activity` + `SystemHealth`):
//
//   * The bootstrap (`WebBootstrap.tui.pool_activity` + `tui.system`) gives
//     the structured first-paint snapshot so the page is never empty.
//   * The WebSocket event spine streams `WebEvent` frames for the scopes
//     `global.activity` (totals + health + bottleneck strings),
//     `pool.{name}` (one `PoolRollup`), and `system.health` (`SystemHealth`).
//
// Keeping the projection here (a pure function of inputs, no React, no store)
// makes the fleet math unit-testable in isolation and keeps `FleetPage.tsx`
// a thin renderer. The slot/utilization/saturation rules mirror the Rust
// `PoolRollup` selectors so the web surface cannot disagree with the TUI.
//
// The implementation is split across cohesive sibling modules; this file is
// the stable barrel so consumers keep importing from './fleetModel':
//   * ./fleetModel.types   — the FleetHealth/FleetPool/FleetTotals/etc. types
//   * ./fleetModel.parse    — defensive JSON primitives (incl. `asHealth`)
//   * ./fleetModel.project — raw PoolRollup/ComponentHealth → typed projections
//   * ./fleetModel.fold    — bootstrap/WS folding + freshness gate

export type {
  FleetHealth,
  FleetPool,
  FleetTotals,
  FleetComponent,
  FleetState,
} from './fleetModel.types';

export { asHealth } from './fleetModel.parse';

export { poolFromRollup, componentsFromSystem } from './fleetModel.project';

export {
  fleetStateFromBootstrap,
  applyFleetEvents,
  isOutOfDate,
} from './fleetModel.fold';
