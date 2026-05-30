// DiffViewer.stories.tsx — diff renderer states (W-T-07).
//
// Covers the five archetypes the plan calls out: small, huge (1000+
// lines), binary, generated, with-comments. The "with-comments" variant
// just demonstrates the inline-comment trigger by passing `onAddComment`.

import type { Meta, StoryObj } from '@storybook/react-vite';
import { useState } from 'react';

import type {
  PullRequestDiffFile,
  PullRequestDiffHunk,
} from '../../api/types';

import { DiffViewer } from './DiffViewer';

const meta: Meta<typeof DiffViewer> = {
  title: 'merge/DiffViewer',
  component: DiffViewer,
};
export default meta;

type Story = StoryObj<typeof DiffViewer>;

const SMALL_HUNK: PullRequestDiffHunk = {
  header: '@@ -1,7 +1,9 @@',
  old_start: 1,
  old_lines: 7,
  new_start: 1,
  new_lines: 9,
  lines: [
    ' use std::collections::HashMap;',
    ' use std::sync::Arc;',
    '-',
    '-pub fn greet(name: &str) {',
    '-    println!("hello, {name}");',
    '+/// New doc-comment.',
    '+pub fn greet(name: &str) -> String {',
    '+    format!("hello, {name}")',
    ' }',
    ' ',
    '+pub fn other() {}',
  ],
};

function smallFile(): PullRequestDiffFile {
  return {
    path: 'src/lib.rs',
    old_path: null,
    status: 'modified',
    additions: 4,
    deletions: 3,
    risk: 'low',
    is_binary: false,
    hunks: [SMALL_HUNK],
  };
}

// Build a single hunk with `count` lines (mix of context + add + del).
function bigHunk(count: number): PullRequestDiffHunk {
  const lines: string[] = [];
  for (let i = 0; i < count; i += 1) {
    if (i % 7 === 0) {
      lines.push(`+pub fn line_${i}() { /* added */ }`);
    } else if (i % 11 === 0) {
      lines.push(`-pub fn line_${i}() { /* removed */ }`);
    } else {
      lines.push(` // line ${i} is context.`);
    }
  }
  return {
    header: `@@ -1,${count} +1,${count} @@`,
    old_start: 1,
    old_lines: count,
    new_start: 1,
    new_lines: count,
    lines,
  };
}

function hugeFile(): PullRequestDiffFile {
  return {
    path: 'src/generated.rs',
    old_path: null,
    status: 'modified',
    additions: 200,
    deletions: 50,
    risk: 'medium',
    is_binary: false,
    hunks: [bigHunk(1000)],
  };
}

function binaryFile(): PullRequestDiffFile {
  return {
    path: 'assets/logo.png',
    old_path: null,
    status: 'modified',
    additions: 0,
    deletions: 0,
    risk: 'low',
    is_binary: true,
    hunks: [],
  };
}

function generatedFile(): PullRequestDiffFile {
  return {
    path: 'contracts/generated/WebBootstrap.ts',
    old_path: null,
    status: 'modified',
    additions: 4,
    deletions: 4,
    risk: 'low',
    is_binary: false,
    hunks: [
      {
        header: '@@ -1,8 +1,8 @@',
        old_start: 1,
        old_lines: 8,
        new_start: 1,
        new_lines: 8,
        lines: [
          ' // GENERATED FILE — DO NOT EDIT.',
          '-export type Foo = string;',
          '+export type Foo = string | number;',
          ' export interface WebBootstrap {',
          '   generated_at: string;',
          '-  schema_version: "0.1.0-alpha";',
          '+  schema_version: "0.2.0";',
          ' }',
        ],
      },
    ],
  };
}

function DiffViewerWithToggle(props: {
  file: PullRequestDiffFile;
  withComments?: boolean;
}): JSX.Element {
  const [mode, setMode] = useState<'unified' | 'split'>('unified');
  return (
    <DiffViewer
      file={props.file}
      mode={mode}
      onModeChange={setMode}
      onAddComment={
        props.withComments
          ? async (_path, _line, _body) => {
              /* no-op for storybook */
            }
          : undefined
      }
    />
  );
}

export const Small: Story = {
  render: () => <DiffViewerWithToggle file={smallFile()} />,
};

export const Huge: Story = {
  render: () => <DiffViewerWithToggle file={hugeFile()} />,
};

export const Binary: Story = {
  render: () => <DiffViewerWithToggle file={binaryFile()} />,
};

export const Generated: Story = {
  render: () => <DiffViewerWithToggle file={generatedFile()} />,
};

export const WithComments: Story = {
  name: 'With comments',
  render: () => <DiffViewerWithToggle file={smallFile()} withComments />,
};
