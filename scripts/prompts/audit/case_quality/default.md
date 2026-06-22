
================================================================
REALISM_RULES (default / utility type — minimal generic checks)
================================================================

R1. Content type coherence.
    - The case should be plausibly a log/snippet of one type. Mixed types
      (e.g. JSON in the middle of CLI output without explanation) -> suspicious.

R2. Line-length entropy.
    - Same as shell R2.

R3. Plausibility of content given the plugin name.
    - `noise_filter_plugin` should actually have noisy content.
    - `ansi_cleaner_plugin` should contain ANSI escape sequences (`\x1b[`).

R4. Encoding consistency.
    - Same as shell R6 (mojibake check).

R5. Empty-output authenticity.
    - Trivially small samples are valid for utility plugins.

R6. No fabricated uniformity.
    - All lines exactly N chars -> fabricated (uniform = LLM-fabricated tell).

R7. Plugin-specific patterns.
    - The plugin's actual job (e.g. encode/decode/clean) should be applicable
      to the content shown.
