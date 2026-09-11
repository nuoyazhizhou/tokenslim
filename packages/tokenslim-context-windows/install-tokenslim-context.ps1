[CmdletBinding()]
param(
    [string]$InstallRoot = (Join-Path $env:LOCALAPPDATA 'TokenSlim'),
    [string]$HostAddress = '127.0.0.1',
    [ValidateRange(1, 65535)]
    [int]$Port = 8765,
    [switch]$EnableTransformation,
    [switch]$Start
)

$ErrorActionPreference = 'Stop'
$packageRoot = Split-Path -Parent $PSCommandPath
$sourceBinary = Join-Path $packageRoot 'bin\tokenslim-server.exe'
if (-not (Test-Path $sourceBinary)) {
    throw "TokenSlim Server binary is missing from this package: $sourceBinary"
}
if ($HostAddress -notin @('127.0.0.1', '::1', 'localhost')) {
    throw 'Only loopback addresses are supported by this installer.'
}

$binRoot = Join-Path $InstallRoot 'bin'
New-Item -ItemType Directory -Force -Path $binRoot | Out-Null
$targetBinary = Join-Path $binRoot 'tokenslim-server.exe'
Copy-Item -Force $sourceBinary $targetBinary

$serverUrl = "http://${HostAddress}:$Port"
[Environment]::SetEnvironmentVariable('TOKENSLIM_SERVER_URL', $serverUrl, 'User')
[Environment]::SetEnvironmentVariable('TOKENSLIM_HOST', $HostAddress, 'User')
[Environment]::SetEnvironmentVariable('TOKENSLIM_PORT', "$Port", 'User')
$env:TOKENSLIM_SERVER_URL = $serverUrl
$env:TOKENSLIM_HOST = $HostAddress
$env:TOKENSLIM_PORT = "$Port"

if ($EnableTransformation) {
    [Environment]::SetEnvironmentVariable('TOKENSLIM_TRANSFORM_ENABLED', 'true', 'User')
    $env:TOKENSLIM_TRANSFORM_ENABLED = 'true'
}

$launcher = Join-Path $InstallRoot 'start-tokenslim-server.ps1'
@"
`$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace(`$env:TOKENSLIM_HOST)) { `$env:TOKENSLIM_HOST = '$HostAddress' }
if ([string]::IsNullOrWhiteSpace(`$env:TOKENSLIM_PORT)) { `$env:TOKENSLIM_PORT = '$Port' }
& '$targetBinary'
"@ | Set-Content -Encoding utf8 $launcher

$started = $false
if ($Start) {
    $existing = Get-NetTCPConnection -LocalPort $Port -ErrorAction SilentlyContinue |
        Where-Object { $_.LocalAddress -in @('127.0.0.1', '::1') } |
        Select-Object -First 1
    if (-not $existing) {
        Start-Process -FilePath $targetBinary -WorkingDirectory $InstallRoot | Out-Null
        $deadline = (Get-Date).AddSeconds(8)
        do {
            Start-Sleep -Milliseconds 250
            try {
                $health = Invoke-WebRequest -UseBasicParsing "$serverUrl/health" -TimeoutSec 1
                if ($health.StatusCode -ge 200 -and $health.StatusCode -lt 300) {
                    $started = $true
                    break
                }
            } catch {
                # The server may still be binding. No request body is sent.
            }
        } while ((Get-Date) -lt $deadline)
    } else {
        $started = $true
    }
}

[pscustomobject]@{
    install_root = $InstallRoot
    server_binary = $targetBinary
    server_url = $serverUrl
    transformation_enabled = [bool]$EnableTransformation
    server_started = $started
} | ConvertTo-Json -Compress
