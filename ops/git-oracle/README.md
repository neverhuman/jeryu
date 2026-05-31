# Git oracle

This directory contains the Phase 1 stock-Git differential harness.

The smoke mode creates a source repo, pushes to `jeryu-gitd` storage through
local Git commands, clones it back, checks object integrity, then runs
`jeryu-mirror import-local` and proves the imported gitd mirror can be cloned
and fetched. The full mode is a placeholder entry point for the expanded P0
matrix: shallow clone, partial clone, LFS, atomic push, force-with-lease,
mirror, submodule, fsck, gc, repack, and bundle.
