# Security Policy

## Supported Versions

| Version | Supported          | Notes                                      |
|---------|--------------------|--------------------------------------------|
| 0.2.6   | :white_check_mark: | First public open-source release           |
| 0.2.5   | :x:                | Deprecated (config flat-layout regression) |
| 0.2.0 – 0.2.4 | :x:          | Deprecated (pre-public-development sample data sanitized) |

Please upgrade to **0.2.6+** by running:

```bash
npm install -g @tokenslim/cli-binary-<your-platform>
# or, via the unified SDK:
npm install -g tokenslim-sdk
```

## Reporting a Vulnerability

Please **do not** open a public issue for security-sensitive findings.  Email
`security@tokenslim.dev` (or open a [GitHub Security Advisory](https://github.com/nuoyazhizhou/tokenslim/security/advisories/new))
with:

1. A clear description of the issue
2. Reproduction steps / minimal sample
3. Affected commit SHA or version tag
4. Potential impact

We aim to acknowledge within 72 hours and ship a fix in the next minor
release (or an out-of-band patch for critical findings).

## Data Hygiene in Public Sample Files

`/samples/` contains synthetic logs used to exercise the compressor's
plugin dispatch.  During pre-public development (in the closed
`TokenSlim` source repository), a subset of `cloud_log_plugin/case_04{5,6,7,8}`
was filled with crawled product listings that contained real merchant
domains and product names.  All such data has been **replaced with
generic, synthetic batch-processing scenarios** in this public release:

- `case_045_aws_logs_insights_table.log` — image batch pipeline
- `case_046_aws_logs_insights_table.log` — CSV → Parquet ETL
- `case_047_aws_logs_insights_table.log` — document parse & index
- `case_048_aws_logs_insights_table.log` — log aggregation analytics

If you find residual real-world data in any file under `/samples/`, please
report it via the channel above; we will sanitize and ship a fix promptly.

## Secrets Policy

This repository **must not** contain:

- API keys, OAuth tokens, npm publish tokens
- Internal hostnames, IP ranges, or credentials
- Real customer / user / device identifiers

Maintainers should manually verify that no commit introduces one of the
above before pushing.  A local grep-based check is recommended (replace
the patterns as needed for the data you handle):

```pwsh
# Adjust the patterns to whatever your organization considers sensitive.
git grep -nE 'AKIA[0-9A-Z]{16}|ghp_[A-Za-z0-9]{36}|xox[baprs]-[A-Za-z0-9-]{10,}' \
  -- 'samples/' 'tests/' '*.scenario.yaml' '*.json'
```

Automated CI gates for sensitive-pattern detection are **not yet
wired in** for this public release.  A `scripts/private-sweep.ps1`
companion utility plus a CI step is planned for 0.2.7+; until then,
the manual check above is the source of truth.
