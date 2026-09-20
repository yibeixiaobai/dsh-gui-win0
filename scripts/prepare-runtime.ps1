$ErrorActionPreference = "Stop"

$manifestPath = Join-Path $PSScriptRoot "..\runtime\dsh-runtime.json"
$manifest = Get-Content $manifestPath | ConvertFrom-Json
$version = $manifest.harnessVersion
$tag = "v$version"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$buildRoot = Join-Path $env:RUNNER_TEMP "dsh-harness-runtime-build"
$archive = Join-Path $buildRoot "deepseek-harness-$version.zip"
$sourceRoot = Join-Path $buildRoot "deepseek-harness-$version"
$targetDir = Join-Path $root "runtime\bin"

Remove-Item $buildRoot -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $buildRoot, $targetDir | Out-Null

$sourceUrl = "https://github.com/deepseek-ai/deepseek-harness/archive/refs/tags/$tag.zip"
Write-Host "Downloading upstream Harness $tag from $sourceUrl"
Invoke-WebRequest -Uri $sourceUrl -OutFile $archive

Expand-Archive -Path $archive -DestinationPath $buildRoot
if (-not (Test-Path (Join-Path $sourceRoot "package.json"))) {
  $sourceRoot = Get-ChildItem $buildRoot -Directory |
    Where-Object { Test-Path (Join-Path $_.FullName "package.json") } |
    Select-Object -First 1 |
    ForEach-Object { $_.FullName }
}
if (-not $sourceRoot) { throw "Upstream Harness source directory was not found." }

Push-Location $sourceRoot
try {
  pnpm install --frozen-lockfile
  pnpm exec tsx scripts/build-exe-for-python-sdk.ts --targets=node24-win-x64
} finally {
  Pop-Location
}

$builtDir = Join-Path $sourceRoot "dist-exe"
$exeName = "deepseek-harness-sdk-runtime-win-x64.exe"
$exe = Join-Path $builtDir $exeName
$rg = Join-Path $builtDir "deepseek-harness-sdk-runtime-win-x64-rg.exe"
$office = Join-Path $sourceRoot "python\sdk-runtime\src\deepseek_harness_runtime\runtime\deepseek-harness-sdk-runtime-win-x64-office"

if (-not (Test-Path $exe)) { throw "Upstream build did not produce $exeName." }
if (-not (Test-Path $rg)) { throw "Upstream build did not produce the Windows ripgrep sidecar." }
if (-not (Test-Path $office)) { throw "Upstream build did not produce the Office sidecar directory." }

Remove-Item (Join-Path $targetDir "*") -Recurse -Force -ErrorAction SilentlyContinue
Copy-Item $exe (Join-Path $targetDir $exeName) -Force
Copy-Item $rg (Join-Path $targetDir (Split-Path $rg -Leaf)) -Force
Copy-Item $office (Join-Path $targetDir (Split-Path $office -Leaf)) -Recurse -Force

$sha = (Get-FileHash (Join-Path $targetDir $exeName) -Algorithm SHA256).Hash.ToLowerInvariant()
$manifest.sha256 = $sha
$manifest | ConvertTo-Json -Depth 8 | Set-Content $manifestPath -Encoding UTF8

Write-Host "Prepared exact upstream Harness runtime $version"
Write-Host "SHA256 $sha"
