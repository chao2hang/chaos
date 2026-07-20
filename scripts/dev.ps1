$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

New-Item -ItemType Directory -Force -Path (Join-Path $Root "data") | Out-Null

$api = Start-Process -FilePath "cargo" -ArgumentList "run -p chaos-api" -WorkingDirectory $Root -PassThru
try {
    pnpm --dir apps/web dev --host 127.0.0.1 --port 5173
}
finally {
    if ($api -and -not $api.HasExited) {
        Stop-Process -Id $api.Id -Force
    }
}
