# VCS Coverage Matrix

## Scope

This matrix tracks practical command coverage between TokenSlim and RTK (embedded reference at `other/rtk`).

## Tools

- TokenSlim: git, svn, hg, p4, cvs, bzr, fossil, darcs
- RTK: deep support for git + gh + gt, no dedicated command modules for svn/hg/p4/cvs/bzr/fossil/darcs

## Command Coverage (TokenSlim)

- git: status, diff, log, show, branch, checkout, switch, merge, rebase, reset, stash, fetch, pull, push, remote, tag, cherry-pick, revert, blame, bisect, restore, clean, submodule
- svn: status, diff, log, info, add, delete, move, copy, commit, update, checkout, revert, merge, switch, resolve, cleanup, blame
- hg: status, diff, log, summary, add, remove, rename, commit, update, branch, pull, push, annotate, graft, rebase, shelve, unshelve, revert
- p4: opened, changes, describe, diff, submit, sync, edit, add, delete, revert, integrate, resolve, reconcile, shelve, unshelve, files, client
- cvs: status, diff, log, add, remove, commit, update, checkout, tag, annotate, edit, unedit, release, history
- bzr: status, diff, log, add, remove, commit, update, branch, pull, push, merge, resolve, missing, revert
- fossil: status, diff, timeline, changes, add, rm, commit, update, sync, checkout, merge, stash, undo, tag
- darcs: whatsnew, diff, changes, record, pull, push, rebase, add, remove, revert, tag, amend-record, obliterate

## RTK Reference (Git only)

From `other/rtk/src/main.rs` + `other/rtk/src/cmds/git/git.rs`:

- git: diff, log, status, show, add, commit, push, pull, branch, fetch, stash, worktree (+ passthrough `Other`)
- gh: dedicated module (`src/cmds/git/gh_cmd.rs`)
- gt: dedicated module (`src/cmds/git/gt_cmd.rs`)

## Priority Gaps

1. TokenSlim: add stronger parse signatures for commit/merge/rebase conflict outputs across non-git VCS.
2. TokenSlim: add semantic event labels in metadata (`op`, `state`, `ref`, `remote`) for downstream summaries.
3. TokenSlim: add regression corpus per command family (status/diff/log/commit/conflict) for all 8 tools.
