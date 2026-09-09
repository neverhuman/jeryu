// tools/toolsHelpers.ts — shared formatters and category display maps for the
// /tools control surface (registry rail, pattern-family dashboard, live scan).

export function formatCount(value: number): string {
  return value.toLocaleString('en-US');
}

export function formatScannedAt(millis: string | null): string {
  if (!millis || !/^[0-9]+$/.test(millis)) return 'never';
  const date = new Date(Number(millis));
  if (Number.isNaN(date.getTime())) return 'never';
  return date.toLocaleString();
}

export const CATEGORY_LABELS: Record<string, string> = {
  'tool-candidate': 'tool candidate',
  'managed-scaffold': 'managed scaffold',
  'config-pattern': 'config pattern',
  'test-pattern': 'test pattern',
};

export function categoryPillClass(category: string): string {
  switch (category) {
    case 'tool-candidate':
      return 'page__pill page__pill--success';
    case 'managed-scaffold':
      return 'page__pill page__pill--warning';
    default:
      return 'page__pill';
  }
}
