// contracts.test-d.ts — type-level drift guard for the generated wire contracts.
//
// `tsd` compiles these assertions against the ts-rs-generated DTOs re-exported
// from the SPA's `api/types` boundary. If the Rust source of truth changes a
// contract shape and the generated TypeScript is regenerated, these assertions
// fail to compile — catching API drift on the web side at the type level
// (complements the Rust `jeryu-readmodel` `contract_drift` test). This lives
// OUTSIDE `src/` so the app's `tsc -b` build ignores its intentional
// `expectError` assertions; `tsd` compiles it on its own. Run: `npm run
// test:contracts`.

import { expectType, expectAssignable, expectError } from 'tsd';

import type {
  MergeSettings,
  RefKind,
  RepositorySettings,
  PullRequestState,
} from '../src/api/types';

declare const merge: MergeSettings;

// Merge policy contract: the booleans + counter the settings studio binds to.
expectType<boolean>(merge.allow_merge_commit);
expectType<boolean>(merge.delete_branch_on_merge);
expectType<boolean>(merge.require_jeryu_merge_passport);
expectType<number>(merge.required_approvals);

// String-union contracts must stay exhaustive.
expectAssignable<RefKind>('branch');
expectAssignable<RefKind>('tag');
expectAssignable<RefKind>('commit');
expectError<RefKind>('not-a-ref-kind');

expectAssignable<PullRequestState>('open');

// RepositorySettings carries the nested merge policy.
declare const settings: RepositorySettings;
expectType<MergeSettings>(settings.merge);
