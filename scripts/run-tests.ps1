param(
  [ValidateSet('deps','start','stop','build','deploy','anchor-test','all-core','all-anchor','ts-test','all')]
  [string]$Command = 'all'
)

$ErrorActionPreference = 'Stop'

$repoWin = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$repoWinUnix = $repoWin -replace '\\', '/'
$repoWslRaw = wsl wslpath -a "$repoWinUnix"
$repoWsl = if ($repoWslRaw) { $repoWslRaw.Trim() } else { '' }

if (-not $repoWsl) {
  throw 'Failed to resolve WSL path for repository.'
}

$rpcUrl = 'http://127.0.0.1:8899'
$ledgerDir = '/tmp/solana-test-ledger'
$pidFile = '/tmp/solana-test-validator.pid'
$faucetPort = '9901'

function Invoke-WslBash {
  param([Parameter(Mandatory = $true)][string]$Script)

  $bashCmd = "cd '$repoWsl' && $Script"
  wsl bash -lc $bashCmd
  if ($LASTEXITCODE -ne 0) {
    throw "WSL command failed: $Script"
  }
}

function Check-WslPrereqs {
  Invoke-WslBash "command -v solana >/dev/null 2>&1 || { echo 'Missing required command: solana' >&2; exit 1; }; command -v anchor >/dev/null 2>&1 || { echo 'Missing required command: anchor' >&2; exit 1; }"
}

function Ensure-NodeDeps {
  if (-not (Test-Path (Join-Path $repoWin 'node_modules'))) {
    Push-Location $repoWin
    try {
      npm install
      if ($LASTEXITCODE -ne 0) {
        throw 'npm install failed'
      }
    }
    finally {
      Pop-Location
    }
  }
}

function Start-Validator {
  $script = 'if solana --url "__RPC__" cluster-version >/dev/null 2>&1; then echo "solana-test-validator already reachable at __RPC__"; exit 0; fi; echo "Starting solana-test-validator (ledger: __LEDGER__)..."; nohup env NO_DNA=1 solana-test-validator --reset --ledger "__LEDGER__" --faucet-port "__FAUCET__" --rpc-port 8899 > /tmp/solana-test-validator.log 2>&1 < /dev/null & echo $! > __PID__; echo "Waiting for RPC __RPC__ to become available..."; for i in {1..60}; do if solana --url "__RPC__" cluster-version >/dev/null 2>&1; then echo "RPC is up"; exit 0; fi; sleep 1; done; echo "Timed out waiting for RPC" >&2; exit 1'
  $script = $script.Replace('__RPC__', $rpcUrl).Replace('__LEDGER__', $ledgerDir).Replace('__FAUCET__', $faucetPort).Replace('__PID__', $pidFile)
  Invoke-WslBash $script
}

function Stop-Validator {
  $script = 'if [ ! -f __PID__ ]; then echo "No PID file found; nothing to stop"; exit 0; fi; pid=$(cat __PID__ 2>/dev/null || true); if [ -z "${pid}" ]; then echo "No PID file found; nothing to stop"; rm -f __PID__; exit 0; fi; echo "Stopping solana-test-validator (pid ${pid})..."; kill "${pid}" || true; rm -f __PID__; echo "Stopped"'
  $script = $script.Replace('__PID__', $pidFile)
  Invoke-WslBash $script
}

function Build-Anchor {
  Invoke-WslBash "echo 'Running anchor build...'; NO_DNA=1 anchor build"
}

function Deploy-Anchor {
  Invoke-WslBash "echo 'Deploying programs with Anchor to $rpcUrl...'; solana -u '$rpcUrl' airdrop 10 >/dev/null 2>&1 || true; NO_DNA=1 ANCHOR_PROVIDER_URL='$rpcUrl' anchor deploy"
}

function Run-AnchorTests {
  Invoke-WslBash "echo 'Running Anchor tests...'; NO_DNA=1 ANCHOR_PROVIDER_URL='$rpcUrl' anchor test --skip-build --skip-local-validator"
}

function Run-TsTests {
  Push-Location $repoWin
  try {
    node .\node_modules\ts-mocha\bin\ts-mocha -p .\tsconfig.json -t 1000000 "tests/**/*.ts"
    if ($LASTEXITCODE -ne 0) {
      throw 'TypeScript tests failed'
    }
  }
  finally {
    Pop-Location
  }
}

switch ($Command) {
  'deps' {
    Ensure-NodeDeps
  }
  'start' {
    Check-WslPrereqs
    Start-Validator
  }
  'stop' {
    Stop-Validator
  }
  'build' {
    Check-WslPrereqs
    Build-Anchor
  }
  'deploy' {
    Check-WslPrereqs
    Deploy-Anchor
  }
  'anchor-test' {
    Check-WslPrereqs
    Run-AnchorTests
  }
  'all-core' {
    Check-WslPrereqs
    try {
      Start-Validator
      Build-Anchor
      Deploy-Anchor
    }
    finally {
      Stop-Validator
    }
  }
  'all-anchor' {
    Check-WslPrereqs
    try {
      Start-Validator
      Build-Anchor
      Deploy-Anchor
      Run-AnchorTests
    }
    finally {
      Stop-Validator
    }
  }
  'ts-test' {
    Ensure-NodeDeps
    Run-TsTests
  }
  'all' {
    Check-WslPrereqs
    Ensure-NodeDeps
    try {
      Start-Validator
      Build-Anchor
      Deploy-Anchor
      Run-TsTests
    }
    finally {
      Stop-Validator
    }
  }
}
