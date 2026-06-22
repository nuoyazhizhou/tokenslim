
================================================================
REALISM_RULES (error_trace type — be strict about stack-trace authenticity)
================================================================

R1. Traceback format coherence.
    - Python: `Traceback (most recent call last):\n  File "...", line N, in ...`
    - Node.js: `at funcName (file:line:col)` and `Error: ...` at top.
    - Java: `Caused by: ...` chains with `at pkg.Class.method(File.java:123)`.
    - Mixing Python and Java stack frame formats in one case -> fabricated.

R2. Stack frame count.
    - Real tracebacks typically have >= 3 frames (the failing frame + 2+ parents).
    - 1-frame "Traceback" with no parent frames -> suspicious.
    - 30+ frames in a non-async case -> suspicious (usually truncated in
      real logs).

R3. File path / module plausibility.
    - Real Python frames reference the actual project modules, not stdlib
      by default unless the bug is in stdlib code.
    - Random `/tmp/` paths in production tracebacks -> suspicious.

R4. Cause chain (for "Caused by" languages).
    - Java: `Caused by:` chains are real and useful; missing them when the
      error is wrapped from a library -> fabricated.
    - Python: `During handling of the above exception, another exception
      occurred:` is real.

R5. Exception type plausibility.
    - `ImportError`, `KeyError`, `ValueError`, `TypeError` are realistic.
    - Random `Error42` exception names -> suspicious.

R6. Empty-output authenticity.
    - Some "errors" are 1-line (e.g. `SyntaxError: invalid syntax at line 42`).
      These are valid but should be flagged as `very_short` in deterministic
      lint, not as fabricated.
    - 0-line error -> fabricated (must have at least an exception type).

R7. Time / context plausibility.
    - Real tracebacks may include timestamps, thread names, request IDs.
    - Missing all of them is OK for short tracebacks; in long multi-line
      ones for production systems, missing all context is suspicious.
