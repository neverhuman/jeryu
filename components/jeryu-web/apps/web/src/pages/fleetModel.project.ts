// fleetModel.project.ts — project raw `PoolRollup` / `ComponentHealth` JSON
// into `FleetPool` / `FleetComponent`, including the derived slot/utilization/
// saturation math that mirrors the Rust `PoolRollup` selectors.

import type { FleetComponent, FleetPool } from './fleetModel.types';
import { asHealth, asRecord, num, str, strList } from './fleetModel.parse';

/** Component order for the system-health strip. */
const COMPONENT_ORDER = ['scm', 'database', 'sandbox', 'cache', 'vault'];

/** Project a raw `PoolRollup` JSON object into a `FleetPool` with derived math. */
export function poolFromRollup(raw: unknown): FleetPool | undefined {
  const r = asRecord(raw);
  if (!r) return;
  const pool = str(r.pool);
  if (!pool) return;
  const queuedJobs = num(r.queued_jobs);
  const runningJobs = num(r.running_jobs);
  const activeSlots = num(r.active_slots);
  const idleSlots = Math.max(0, activeSlots - runningJobs);
  const utilization =
    activeSlots === 0 ? 0 : Math.min(1, Math.max(0, runningJobs / activeSlots));
  return {
    pool,
    tags: strList(r.tags),
    trustTier: str(r.trust_tier) || 'trusted',
    paused: r.paused === true,
    queuedJobs,
    runningJobs,
    failedJobs: num(r.failed_jobs),
    activeSlots,
    configuredMaxSlots: num(r.configured_max_slots),
    onlineRunners: num(r.online_runners),
    stuckRunners: num(r.stuck_runners),
    idleSlots,
    utilization,
    saturated: queuedJobs > 0 && idleSlots === 0,
  };
}

/** Project a raw `ComponentHealth` JSON object into a `FleetComponent`. */
function componentFromRaw(name: string, raw: unknown): FleetComponent {
  const r = asRecord(raw);
  const latency = r ? r.latency_ms : null;
  const detail = r ? r.detail : null;
  return {
    name: r && typeof r.name === 'string' ? r.name : name,
    status: asHealth(r?.status),
    latencyMs:
      typeof latency === 'number' && Number.isFinite(latency) ? latency : null,
    detail: typeof detail === 'string' ? detail : null,
  };
}

/** Read the ordered scm/database/sandbox/cache/vault strip from a `SystemHealth`. */
export function componentsFromSystem(raw: unknown): FleetComponent[] {
  const sys = asRecord(raw);
  if (!sys) return [];
  return COMPONENT_ORDER.filter((name) => name in sys).map((name) =>
    componentFromRaw(name, sys[name])
  );
}
