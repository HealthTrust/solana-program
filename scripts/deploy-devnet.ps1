param(
  [string]$RpcUrl = 'https://api.devnet.solana.com',
  [string]$Wallet = '~/.config/solana/id.json',
  [ValidateSet('all','data_registry','order_handler')]
  [string]$Program = 'all',
  [switch]$SkipBuild,
  [switch]$SkipAirdrop,
  [switch]$SyncIdls
)

$ErrorActionPreference = 'Stop'

$repoWin = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$repoWinUnix = $repoWin -replace '\\', '/'
$repoWslRaw = wsl wslpath -a "$repoWinUnix"
$repoWsl = if ($repoWslRaw) { $repoWslRaw.Trim() } else { '' }

if (-not $repoWsl) {
  throw 'Failed to resolve WSL path for solana-program.'
}

function Invoke-WslBash {
  param([Parameter(Mandatory = $true)][string]$Script)

  $Script = $Script.Replace("`r", "")
  $bytes = [System.Text.Encoding]::UTF8.GetBytes("export PATH=`"`$HOME/.local/share/solana/install/active_release/bin:`$HOME/.cargo/bin:`$PATH`"`ncd '$repoWsl'`n$Script")
  $b64 = [Convert]::ToBase64String($bytes)
  
  wsl bash -c "echo $b64 | base64 -d | bash"
  if ($LASTEXITCODE -ne 0) {
    throw "WSL command failed: $Script"
  }
}

function Check-Prereqs {
  $script = @'
set -e
WALLET="__WALLET__"
WALLET="${WALLET/#\~/$HOME}"
command -v solana >/dev/null 2>&1 || { echo 'Missing required command: solana' >&2; exit 1; }
command -v anchor >/dev/null 2>&1 || { echo 'Missing required command: anchor' >&2; exit 1; }
test -f "$WALLET" || { echo "Wallet keypair not found: $WALLET" >&2; exit 1; }
echo "Solana: $(solana --version)"
echo "Anchor: $(anchor --version)"
echo "Wallet: $(solana-keygen pubkey "$WALLET")"
'@
  Invoke-WslBash ($script.Replace('__WALLET__', $Wallet))
}

function Configure-Devnet {
  $script = @'
set -e
WALLET="__WALLET__"
WALLET="${WALLET/#\~/$HOME}"
solana config set --url '__RPC_URL__' --keypair "$WALLET"
solana config get
'@
  Invoke-WslBash ($script.Replace('__RPC_URL__', $RpcUrl).Replace('__WALLET__', $Wallet))
}

function Ensure-Balance {
  if ($SkipAirdrop) {
    $script = @'
set -e
WALLET="__WALLET__"
WALLET="${WALLET/#\~/$HOME}"
echo 'Skipping airdrop'
solana balance --url '__RPC_URL__' --keypair "$WALLET"
'@
    Invoke-WslBash ($script.Replace('__RPC_URL__', $RpcUrl).Replace('__WALLET__', $Wallet))
    return
  }

  $script = @'
set -e
WALLET="__WALLET__"
WALLET="${WALLET/#\~/$HOME}"
balance=$(solana balance --url '__RPC_URL__' --keypair "$WALLET" | awk '{print $1}')
echo "Current devnet balance: ${balance} SOL"
if awk "BEGIN { exit !($balance < 2.5) }"; then
  echo 'Requesting devnet airdrop...'
  solana airdrop 1 --url '__RPC_URL__' --keypair "$WALLET" || true
  sleep 2
  solana airdrop 0.5 --url '__RPC_URL__' --keypair "$WALLET" || true
  solana balance --url '__RPC_URL__' --keypair "$WALLET"
fi
'@
  Invoke-WslBash ($script.Replace('__RPC_URL__', $RpcUrl).Replace('__WALLET__', $Wallet))
}

function Build-Programs {
  if ($SkipBuild) {
    Write-Host 'Skipping anchor build'
    return
  }

  Invoke-WslBash "set -e; export NO_DNA=1; anchor build"
}

function Deploy-Programs {
  $programArg = if ($Program -eq 'all') { '' } else { "--program-name '$Program'" }
  $script = @'
set -e
WALLET="__WALLET__"
WALLET="${WALLET/#\~/$HOME}"
export NO_DNA=1 ANCHOR_PROVIDER_URL='__RPC_URL__' ANCHOR_WALLET="$WALLET"
anchor deploy __PROGRAM_ARG__ --provider.cluster devnet --provider.wallet "$WALLET"
'@
  $script = $script.Replace('__RPC_URL__', $RpcUrl).Replace('__WALLET__', $Wallet).Replace('__PROGRAM_ARG__', $programArg)
  if ($Program -eq 'all') {
    Invoke-WslBash $script
    return
  }

  Invoke-WslBash $script
}

function Show-DeployedPrograms {
  $script = @'
set -e
echo 'Configured devnet program IDs:'
anchor keys list
echo ''
echo 'Devnet program account checks:'
for program_id in 3zmhW1fxXXGKCn31Uz8BaZ34gmNRGgAG6LFk1P6gWkDT GVUZtHZHr1tDxw3Pt142BxqgkS3dfDpPbqEznsFT9jV4; do
  echo "--- $program_id"
  solana account "$program_id" --url '__RPC_URL__' --output json >/dev/null && echo 'exists on devnet' || echo 'not found on devnet'
done
'@
  Invoke-WslBash ($script.Replace('__RPC_URL__', $RpcUrl))
}

function Sync-Idls {
  if (-not $SyncIdls) {
    return
  }

  $frontendIdlDir = Join-Path $repoWin '..\MVP-Frontend\src\idls'
  $backendIdlDir = Join-Path $repoWin '..\MVP-Server-Backend\idls'

  New-Item -ItemType Directory -Force -Path $frontendIdlDir | Out-Null
  New-Item -ItemType Directory -Force -Path $backendIdlDir | Out-Null

  Copy-Item -LiteralPath (Join-Path $repoWin 'target\idl\data_registry.json') -Destination $frontendIdlDir -Force
  Copy-Item -LiteralPath (Join-Path $repoWin 'target\idl\order_handler.json') -Destination $frontendIdlDir -Force
  Copy-Item -LiteralPath (Join-Path $repoWin 'target\idl\data_registry.json') -Destination $backendIdlDir -Force
  Copy-Item -LiteralPath (Join-Path $repoWin 'target\idl\order_handler.json') -Destination $backendIdlDir -Force

  Write-Host 'Synced IDLs to frontend and backend.'
}

Check-Prereqs
Configure-Devnet
Ensure-Balance
Build-Programs
Deploy-Programs
Show-DeployedPrograms
Sync-Idls
