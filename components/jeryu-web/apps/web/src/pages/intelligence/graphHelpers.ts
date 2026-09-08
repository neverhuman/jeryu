// graphHelpers.ts - pure geometry/label helpers for the operator graph views.

import type { OperatorGraphNode } from '../intelligenceGraphModel';

export function nodeRadius(node: OperatorGraphNode): number {
  return Math.min(12, Math.max(5, 5 + node.weight));
}

export function compactLabel(label: string): string {
  const tail = label.split('/').at(-1) ?? label;
  return tail.length > 18 ? `${tail.slice(0, 15)}...` : tail;
}

export function diamondPoints(x: number, y: number, r: number): string {
  return `${x},${y - r} ${x + r},${y} ${x},${y + r} ${x - r},${y}`;
}

export function hexPoints(x: number, y: number, r: number): string {
  const dx = r * 0.86;
  const half = r / 2;
  return [
    `${x - dx},${y - half}`,
    `${x},${y - r}`,
    `${x + dx},${y - half}`,
    `${x + dx},${y + half}`,
    `${x},${y + r}`,
    `${x - dx},${y + half}`,
  ].join(' ');
}

export function toggle<T extends string>(items: T[], value: T): T[] {
  return items.includes(value)
    ? items.filter((item) => item !== value)
    : [...items, value];
}
