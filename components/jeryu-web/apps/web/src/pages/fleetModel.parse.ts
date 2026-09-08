// fleetModel.parse.ts — defensive primitives for reading untyped WS/bootstrap
// JSON into typed values. Shared by the projection and fold sibling modules;
// only `asHealth` is re-exported from ./fleetModel as public API.

import type { FleetHealth } from './fleetModel.types';

const HEALTH_VALUES: ReadonlySet<string> = new Set([
  'healthy',
  'warning',
  'degraded',
  'critical',
  'unknown',
]);

export function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null
    ? (value as Record<string, unknown>)
    : null;
}

export function num(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

export function str(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

export function strList(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : [];
}

export function asHealth(value: unknown): FleetHealth {
  return typeof value === 'string' && HEALTH_VALUES.has(value)
    ? (value as FleetHealth)
    : 'unknown';
}
