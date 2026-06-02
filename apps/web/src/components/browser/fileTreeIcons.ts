// fileTreeIcons.ts — extension → icon mapping for the file browser.
//
// Shared by the lazy tree (`FileTree`) and the flat virtualized variant
// (`FlatFileList`) so both pick the same icon for a given filename:
//   .md  → FileText
//   code extensions → FileCode
//   anything else → File
// (Directory icons are chosen at the row level since they depend on the
// open/closed state.)

import { File, FileCode, FileText, type LucideIcon } from 'lucide-react';

export const ROW_HEIGHT_PX = 28;
export const INDENT_PX = 16;

const CODE_EXTENSIONS = new Set([
  'ts',
  'tsx',
  'js',
  'jsx',
  'mjs',
  'cjs',
  'rs',
  'py',
  'go',
  'java',
  'kt',
  'c',
  'cc',
  'cpp',
  'cxx',
  'h',
  'hpp',
  'hh',
  'cs',
  'php',
  'scala',
  'swift',
  'sh',
  'bash',
  'zsh',
  'fish',
  'lua',
  'sql',
  'json',
  'toml',
  'yaml',
  'yml',
]);

export function fileIcon(name: string): LucideIcon {
  const lower = name.toLowerCase();
  if (lower.endsWith('.md') || lower.endsWith('.markdown')) return FileText;
  const ext = lower.split('.').pop();
  if (ext && CODE_EXTENSIONS.has(ext)) return FileCode;
  return File;
}
