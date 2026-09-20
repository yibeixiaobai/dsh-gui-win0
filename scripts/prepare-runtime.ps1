$ErrorActionPreference = "Stop"

$manifestPath = Join-Path $PSScriptRoot "..\runtime\dsh-runtime.json"
$manifest = Get-Content $manifestPath | ConvertFrom-Json
$version = $manifest.harnessVersion
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$buildDir = Join-Path $root ".desktop-build\runtime"
$downloadDir = Join-Path $buildDir "download"
$extractDir = Join-Path $buildDir "extract"
$targetDir = Join-Path $root "runtime\bin"

New-Item -ItemType Directory -Force $downloadDir, $extractDir, $targetDir | Out-Null
python -m pip download --no-deps "deepseek-harness-runtime-bin==$version" --dest $downloadDir

$wheel = Get-ChildItem $downloadDir -Filter "*.whl" | Select-Object -First 1
if (-not $wheel) { throw "Runtime wheel was not downloaded." }

Remove-Item $extractDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $extractDir | Out-Null
Expand-Archive -Path $wheel.FullName -DestinationPath $extractDir

$exe = Get-ChildItem $extractDir -Recurse -Filter "deepseek-harness-sdk-runtime-windows-x64.exe" | Select-Object -First 1
if (-not $exe) { throw "Windows x64 runtime executable not found in wheel." }
Copy-Item $exe.FullName (Join-Path $targetDir $exe.Name) -Force

$office = Get-ChildItem $extractDir -Recurse -Directory | Where-Object { $_.Name -eq "deepseek-harness-sdk-runtime-windows-x64-office" } | Select-Object -First 1
if ($office) { Copy-Item $office.FullName $targetDir -Recurse -Force }

$rg = Get-ChildItem $extractDir -Recurse -Filter "deepseek-harness-sdk-runtime-windows-x64-rg.exe" | Select-Object -First 1
if ($rg) { Copy-Item $rg.FullName (Join-Path $targetDir $rg.Name) -Force }

Write-Host "Prepared pinned Harness runtime $version"
