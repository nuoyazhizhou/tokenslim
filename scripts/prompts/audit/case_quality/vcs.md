
================================================================
REALISM_RULES (vcs type — be strict about VCS command output authenticity)
================================================================

R1. Command header coherence.
    - The first line should clearly be a VCS command (git/svn/hg/p4/...)
      or a recognized output (commit message, status line, log entry).
    - Mixing git status output with hg changeset format -> fabricated.

R2. Hash / revision plausibility.
    - git commit hash: 7-40 hex chars.
    - svn revision: integer.
    - hg changeset: 12-40 hex chars (default 12).
    - p4 changelist: integer in `change N` form.
    - Random non-hex characters in a git hash -> fabricated.

R3. Branch / ref naming.
    - Real branches: `main`, `master`, `develop`, `feature/foo`, `release/1.x`.
    - Random `asdf1234` branch names -> suspicious.
    - Refs with spaces or special chars without escaping -> suspicious.

R4. File path plausibility.
    - File paths in status/log output should be repo-relative, not absolute
      `/Users/x/` or `C:\x\`.
    - Backslash paths in git output (git always uses forward slashes) -> fabricated.

R5. Output verbosity consistency.
    - `git status` is short (under 50 lines usually).
    - `git log` can be long but each entry has consistent format.
    - `svn diff` can be very long but has `Index: ...` + `--- / +++` markers.
    - A "git log" entry that looks like `svn log` output -> fabricated.

R6. Diff format fidelity.
    - Unified diff has `@@ -X,Y +A,B @@` hunk headers.
    - Real diff lines start with ` `, `+`, `-` (or `\\` for "no newline").
    - Diffs with `>>>` instead of `+++` -> fabricated.

R7. VCS tool / repo alignment.
    - `git` commands referring to a SVN-style revision number (e.g. `r1234`)
      without git-svn context -> fabricated.
    - `hg log` output with `commit` (git term) instead of `changeset` -> fabricated.
    - p4 commands on a non-p4 server hint -> fabricated.
