// fleetModel.types.ts — the normalized, render-ready fleet view types.
//
// See ./fleetModel.ts for the module overview. These interfaces mirror the
// Rust read-model contract (`jeryu_readmodel::pool_activity` + `SystemHealth`)
// and are the shared vocabulary of the projection/fold sibling modules.

/** Health level mirrors `jeryu_readmodel::HealthLevel` (snake_case wire). */
export type FleetHealth =
  | 'healthy'
  | 'warning'
  | 'degraded'
  | 'critical'
  | 'unknown';

/** A single runner pool, rolled up across every repo on the server. */
export interface FleetPool {
  pool: string;
  tags: string[];
  trustTier: string;
  paused: boolean;
  queuedJobs: number;
  runningJobs: number;
  failedJobs: number;
  activeSlots: number;
  configuredMaxSlots: number;
  onlineRunners: number;
  stuckRunners: number;
  /** Derived: slots free for new work (active − running, floored at 0). */
  idleSlots: number;
  /** Derived: running / active, clamped 0..1 (0 when no active slots). */
  utilization: number;
  /** Derived: queued work with no idle slot to take it. */
  saturated: boolean;
}

/** Server-wide totals (mirrors `ActivityTotals`). */
export interface FleetTotals {
  repos: number;
  pools: number;
  queuedJobs: number;
  runningJobs: number;
  failedJobs: number;
  onlineRunners: number;
  stuckRunners: number;
}

/** One system-health component (mirrors `ComponentHealth`). */
export interface FleetComponent {
  name: string;
  status: FleetHealth;
  latencyMs: number | null;
  detail: string | null;
}

/** The fully-folded view the page renders. */
export interface FleetState {
  /** Overall pool-fabric health. */
  health: FleetHealth;
  totals: FleetTotals;
  pools: FleetPool[];
  /** Human-readable bottleneck lines, most severe first. */
  bottlenecks: string[];
  /** scm/database/sandbox/cache/vault component health. */
  components: FleetComponent[];
  /** Online-but-stuck runner total across the server. */
  stuckRunnerTotal: number;
  /** Fleet-wide utilization (running / active across all pools), 0..1. */
  utilization: number;
  /** Most recent timestamp seen across the inputs, or null if none. */
  lastUpdated: string | null;
}
