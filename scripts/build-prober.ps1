$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$ToolDir = Join-Path $Root "tools/chaos-prober"
$OutDir = Join-Path $Root "third_party/chaos-prober"

if (-not (Get-Command go -ErrorAction SilentlyContinue)) {
    throw "Go is required to build chaos-prober"
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Push-Location $ToolDir
try {
    go build -trimpath -o (Join-Path $OutDir "chaos-prober.exe") .
}
finally {
    Pop-Location
}
