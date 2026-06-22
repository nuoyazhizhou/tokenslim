================================================================
REALISM_RULES (the actual reason for LLM invocation — be strict)
================================================================

R1. Shell personality coherence.
    - The prompt format and error format must be from the same shell.
    - Contradictions (any of these = fabricated):
      * zsh prompt (`%` or `❯`) + bash error (`bash: ...`) -> fabricated
      * fish prompt (`>`) + bash error -> fabricated
      * PowerShell prompt (`PS >`) + Unix-style error message -> fabricated
      * bash prompt + Windows-style path `C:\foo` (without `MSYS`/`Cygwin` prefix) -> fabricated

R2. Line-length entropy.
    - Real terminal output has mixed line widths.
    - Tells (any one = fabricated):
      * Every non-empty line is exactly N chars wide.
      * All error messages have identical column alignment.
      * All output lines end at the same column.

R3. Error cause-effect chain.
    - Every `Permission denied` / `command not found` / `Connection refused`
      must be traceable to a plausible preceding command in the case.
    - Tells (any one = fabricated):
      * `cp` fails with `No such file` but the source was just listed
        in the previous `ls` output (logical contradiction).
      * `python foo.py` fails with `ModuleNotFoundError` but the case
        never had a `pip install` step (unmotivated dependency miss).
      * `ssh` fails with `Connection refused` but the case never tried
        to start a server.

R4. Error multi-line fidelity.
    - Stack traces, npm errors, gcc errors, python tracebacks typically
      have multiple lines.
    - Tells (any one = fabricated):
      * Python `Traceback` is 1 line only (real ones are >= 3).
      * `npm ERR!` is a single line (real ones have 3-5 lines).
      * `gcc` error is 1 line (real ones have caret + hint).
      * `systemctl status foo` shows failure but no `Loaded` / `Active`
        / `Main PID` block (real status has 6+ lines).

R5. Empty-output authenticity.
    - `cd`, `mkdir`, `cp`, `mv`, `rm`, `touch`, `Copy-Item`, `Remove-Item`
      naturally produce no stdout on success.
    - Tells (any one = fabricated):
      * `mkdir foo` followed by "Successfully created directory" or
        "1 directory created" (artificial confirmation).
      * `rm file.txt` followed by "File removed" (real rm is silent).
      * `cp a b` followed by "1 file(s) copied" in PowerShell - wait,
        this IS the real PS output. So this is the inverse: real PS
        `Copy-Item` does print this, so DO NOT flag it. Use your
        judgment: is the empty/silent-vs-noisy contract correct for
        the specific tool?

R6. Exit code vs stderr consistency.
    - exit 0 + nonzero stderr (warnings) is normal (`rm` on missing,
      `grep` no match, deprecated warnings).
    - exit != 0 + empty stderr is rare; usually has at least one error line.
    - Tells (any one = fabricated):
      * `errorlevel 1` printed but no preceding error message.
      * `exit 127` (command not found) with no error before it.
      * The case shows a successful-looking command but then exits
        with a non-zero code, with no stderr (unmotivated failure).

R7. Locale/region plausibility.
    - The toolset must match the OS implied by the prompt.
    - Tells (any one = fabricated):
      * `user@host:~$ brew install` (macOS tool on Linux prompt).
      * `PS C:\Users\foo> apt-get` (Debian tool on Windows prompt).
      * `user@host % pacman -S` (Arch tool on macOS prompt).
      * `C:\> ls` (Unix command on Windows prompt without WSL/MSYS hint).

================================================================
DECISION DISCIPLINE
================================================================

- If ANY of R1-R7 has a clear tell, choose status="fabricated" AND list
  the specific tells in `realism_audit.fabrication_indicators`.
- A single tell is enough; do not require multiple tells.
- "fabricated" can coexist with a structurally valid case: a case can
  pass S1-S6 and still be fabricated per R1-R7. In that case, return
  status="fabricated" and let the rest of the audit surface both
  verdicts separately.
- "fabricated" is stronger than "valid" but is a content-level signal;
  routing / not_registered / duplicate issues still take priority in
  the synthesis step.

LLM rule:
- Return JSON only, no markdown wrapper.
- Do not edit the case; only judge.

Required JSON schema:
{
  "status": "one of the nine labels (now including 'fabricated')",
  "confidence": 0.0-1.0,
  "explanation": "Chinese explanation, 1-3 sentences",
  "fix_hint": "short patch hint or empty string",
  "duplicate_of": "case_xxx_... or empty",
  "realism_audit": {
    "shell_personality_ok": bool,
    "line_length_entropy_ok": bool,
    "error_cause_effect_ok": bool,
    "error_multiline_ok": bool,
    "empty_output_authentic": bool,
    "exit_stderr_consistent": bool,
    "locale_plausible": bool,
    "fabrication_indicators": ["specific tell #1", "specific tell #2", ...]
  }
}
