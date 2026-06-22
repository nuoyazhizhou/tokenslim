
================================================================
REALISM_RULES (data_struct type — be strict about format validity)
================================================================

R1. Format consistency.
    - The case must be parseable as the declared format (YAML/JSON/XML/etc).
    - Mixed YAML + JSON in one case -> fabricated.
    - XML with mismatched open/close tags -> fabricated.
    - JSON with trailing commas -> fabricated (real JSON.parse would fail).

R2. Indentation / structure.
    - YAML uses spaces (NOT tabs) for indentation. Tabs in YAML -> fabricated.
    - Real YAML rarely mixes 2-space and 4-space indent in one case.
    - JSON is uniformly indented; random indent in one JSON file -> fabricated.

R3. Key naming plausibility.
    - `name`, `version`, `id`, `created_at` are realistic.
    - `x_j4f_99q` random gibberish keys (not just snake_case) -> suspicious.

R4. Field coherence.
    - In a k8s manifest, `apiVersion: v1` should match `kind: Pod/Service/...`.
    - In a docker-compose, `services:` should have at least one service entry.
    - Empty `services:` or `image:` with no value -> fabricated.

R5. Empty-output authenticity.
    - Empty `{}`, empty `<root></root>`, single-key `{key: value}` are all
      real and authentic. They are NOT too-small in this type.

R6. Encoding consistency.
    - UTF-8 BOM in JSON sometimes (e.g. Node), but mixing UTF-8 + cp1252 in
      one file -> fabricated.
    - Real YAML allows `\n` in flow style; real JSON requires `\n` to be
      escaped as `\\n` inside strings.

R7. Realistic data ranges.
    - Version strings like `1.0.0`, `2.13.4`, `v3.0.0-rc.1` are realistic.
    - Random hex blobs as version values -> suspicious.
    - Timestamps like `2026-05-07T10:33:44Z` are realistic.
    - Epoch `0` for a `created_at` without explanation -> suspicious.
