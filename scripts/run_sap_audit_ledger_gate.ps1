[CmdletBinding()]
param(
    [int]$ExpectedCount = 0,
    [int]$MaxErrors = 80
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$validator = Join-Path $PSScriptRoot 'validate_sap_audit_ledger.py'
if (-not (Test-Path -LiteralPath $validator -PathType Leaf)) {
    throw "SAP audit validator not found: $validator"
}

Push-Location $repoRoot
try {
    $arguments = @('scripts/validate_sap_audit_ledger.py', '--max-errors', $MaxErrors)
    if ($ExpectedCount -gt 0) {
        $arguments += @('--expected-count', $ExpectedCount)
    }
    & python @arguments
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
