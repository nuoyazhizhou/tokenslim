
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
  },
  "sidecar_accuracy": {
    "scenario_accurate": bool,
    "target_capability_accurate": bool,
    "expected_keep_accurate": bool,
    "expected_compress_accurate": bool,
    "inaccurate_fields": ["field name that is inaccurate", ...]
  }
}
