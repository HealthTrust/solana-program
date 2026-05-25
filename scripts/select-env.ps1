param(
  [Parameter(Mandatory = $true)]
  [ValidateSet('dev', 'staging', 'prod', 'prod-temporary')]
  [string]$Environment,

  [switch]$SkipKeypairCopy
)

$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$configPath = Join-Path $repo "env\$Environment.json"

if (-not (Test-Path -LiteralPath $configPath)) {
  throw "Environment config not found: $configPath"
}

$config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json

function Require-ProgramId {
  param([string]$Name, [string]$Value)

  if (-not $Value -or $Value -notmatch '^[1-9A-HJ-NP-Za-km-z]{32,44}$') {
    throw "$Name is not a valid-looking Solana public key: $Value"
  }
}

function Replace-FileText {
  param(
    [string]$Path,
    [string]$Pattern,
    [string]$Replacement
  )

  $text = Get-Content -LiteralPath $Path -Raw
  if (-not [regex]::IsMatch($text, $Pattern)) {
    throw "Pattern did not match in $Path"
  }
  $updated = [regex]::Replace($text, $Pattern, $Replacement)
  Set-Content -LiteralPath $Path -Value $updated -NoNewline
}

function Convert-ToWslPath {
  param([string]$Path)

  $resolved = (Resolve-Path -LiteralPath $Path).Path
  $unixish = $resolved.Replace('\', '/')
  if ($unixish -match '^([A-Za-z]):/(.*)$') {
    return "/mnt/$($matches[1].ToLower())/$($matches[2])"
  }
  return $unixish
}

function Get-SolanaKeypairPubkey {
  param([string]$Path)

  $wslPath = Convert-ToWslPath $Path
  $script = @"
set -e
export PATH="`$HOME/.local/share/solana/install/active_release/bin:`$HOME/.cargo/bin:`$PATH"
solana-keygen pubkey '$wslPath'
"@
  $bytes = [System.Text.Encoding]::UTF8.GetBytes($script.Replace("`r", ""))
  $b64 = [Convert]::ToBase64String($bytes)
  $pubkey = & wsl bash -lc "echo $b64 | base64 -d | bash"
  if ($LASTEXITCODE -ne 0 -or -not $pubkey) {
    throw "Failed to read keypair public key: $Path"
  }
  return $pubkey.Trim()
}

Require-ProgramId 'dataRegistryProgramId' $config.dataRegistryProgramId
Require-ProgramId 'orderHandlerProgramId' $config.orderHandlerProgramId

$anchorToml = Join-Path $repo 'Anchor.toml'
$dataRegistryLib = Join-Path $repo 'programs\data_registry\src\lib.rs'
$orderHandlerLib = Join-Path $repo 'programs\order_handler\src\lib.rs'

Replace-FileText `
  -Path $anchorToml `
  -Pattern '(?m)^(data_registry\s*=\s*")[^"]+(")$' `
  -Replacement "`${1}$($config.dataRegistryProgramId)`${2}"

Replace-FileText `
  -Path $anchorToml `
  -Pattern '(?m)^(order_handler\s*=\s*")[^"]+(")$' `
  -Replacement "`${1}$($config.orderHandlerProgramId)`${2}"

Replace-FileText `
  -Path $dataRegistryLib `
  -Pattern 'declare_id!\("[^"]+"\);' `
  -Replacement "declare_id!(`"$($config.dataRegistryProgramId)`");"

Replace-FileText `
  -Path $orderHandlerLib `
  -Pattern 'declare_id!\("[^"]+"\);' `
  -Replacement "declare_id!(`"$($config.orderHandlerProgramId)`");"

if (-not $SkipKeypairCopy) {
  if ($config.dataRegistryKeypair -and $config.orderHandlerKeypair) {
    $dataKeypairPath = Join-Path $repo $config.dataRegistryKeypair
    $orderKeypairPath = Join-Path $repo $config.orderHandlerKeypair
    $targetDeploy = Join-Path $repo 'target\deploy'
    New-Item -ItemType Directory -Force -Path $targetDeploy | Out-Null

    Copy-Item -LiteralPath $dataKeypairPath -Destination (Join-Path $targetDeploy 'data_registry-keypair.json') -Force
    Copy-Item -LiteralPath $orderKeypairPath -Destination (Join-Path $targetDeploy 'order_handler-keypair.json') -Force

    $dataPubkey = Get-SolanaKeypairPubkey (Join-Path $targetDeploy 'data_registry-keypair.json')
    $orderPubkey = Get-SolanaKeypairPubkey (Join-Path $targetDeploy 'order_handler-keypair.json')

    if ($dataPubkey -ne $config.dataRegistryProgramId) {
      throw "data_registry keypair mismatch. Expected $($config.dataRegistryProgramId), got $dataPubkey"
    }
    if ($orderPubkey -ne $config.orderHandlerProgramId) {
      throw "order_handler keypair mismatch. Expected $($config.orderHandlerProgramId), got $orderPubkey"
    }
  } else {
    Write-Warning "No program keypairs configured for '$Environment'; skipped target/deploy keypair copy."
  }
}

Write-Host "Selected Solana environment: $Environment"
Write-Host "cluster: $($config.cluster)"
Write-Host "data_registry: $($config.dataRegistryProgramId)"
Write-Host "order_handler: $($config.orderHandlerProgramId)"
