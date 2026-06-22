
================================================================
REALISM_RULES (access_log type — be strict about HTTP log authenticity)
================================================================

R1. Log line coherence.
    - The case must look like an access log (nginx CLF / JSON / W3C / cloud
      format) and stick to one format. Contradictions = fabricated:
      * Mixed CLF + W3C fields in the same case -> fabricated.
      * Combined + Common format mixed (e.g. half of the case has referer,
        half missing without reason) -> fabricated.

R2. Line-length entropy.
    - Same as shell R2 — real logs have variable-width lines.

R3. IP / timestamp plausibility.
    - Public IPs in private ranges (10/8, 172.16/12, 192.168/16) in
      "public" access cases -> fabricated. RFC 1918 reserved ranges should
      be private.
    - Future timestamps beyond today's date -> fabricated.
    - Out-of-order timestamps without explanation -> fabricated.

R4. HTTP status code plausibility.
    - Status code must match the request context:
      * `200` after a "POST /api/users" with no `Content-Length` mismatch -> plausible.
      * `200` after a "POST /api/checkout" that should create -> plausible.
      * `500` without an `error` / `Exception` body -> fabricated.
      * `404` for a static asset URL that never existed in the case -> fabricated.
      * `4xx` codes (401/403) with no auth-related headers in request -> fabricated.

R5. Empty-output authenticity.
    - 1-line access logs are common and authentic.
    - 0-line or "commented out" cases are suspicious.

R6. Field format consistency.
    - Each log format has a fixed field order/separator:
      * CLF: IP - - [date:tz] "method path HTTP/x.x" status size "ref" "ua"
      * JSON: {"timestamp": ..., "status": ...}
      * W3C: #Fields: ...\ndate time s-ip ...
    - Mixing fields from different formats within one case -> fabricated.

R7. UA / referer plausibility.
    - `User-Agent: curl/8.7.1` is a real UA; `User-Agent: Browser/0.0.0` is
      suspicious.
    - Referer pointing to a domain unrelated to the Host header -> suspicious.
    - Bot UAs (`Googlebot`, `bingbot`) hitting login pages -> suspicious.
