$ErrorActionPreference = "Stop"

$manifestPath = Join-Path $PSScriptRoot "..\runtime\dsh-runtime.json"
$manifest = Get-Content $manifestPath | ConvertFrom-Json
$version = $manifest.harnessVersion
$wheelVersion = $version -replace "-alpha\.", "a" -replace "-beta\.", "b" -replace "-rc\.", "rc"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$buildDir = Join-Path $root ".desktop-build\runtime"
$downloadDir = Join-Path $buildDir "download"
$extractDir = Join-Path $buildDir "extract"
$targetDir = Join-Path $root "runtime\bin"

Remove-Item $downloadDir, $extractDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $downloadDir, $extractDir, $targetDir | Out-Null

python -m pip download --disable-pip-version-check --no-deps "deepseek-harness-runtime-bin==$wheelVersion" --dest $downloadDir

$wheel = Get-ChildItem $downloadDir -Filter "*.whl" | Select-Object -First 1
if (-not $wheel) { throw "Runtime wheel was not downloaded." }

Expand-Archive -Path $wheel.FullName -DestinationPath $extractDir

$exeName = "deepseek-harness-sdk-runtime-win-x64.exe"
$exe = Get-ChildItem $extractDir -Recurse -Filter $exeName | Select-Object -First 1
if (-not $exe) { throw "Expected runtime executable $exeName was not found in wheel." }

$rg = Get-ChildItem $extractDir -Recurse -Filter "deepseek-harness-sdk-runtime-win-x64-rg.exe" | Select-Object -First 1
$office = Get-ChildItem $extractDir -Recurse -Directory -Filter "deepseek-harness-sdk-runtime-win-x64-office" | Select-Object -First 1
if (-not $rg) { throw "Expected ripgrep sidecar was not found in wheel." }
if (-not $office) { throw "Expected Office sidecar directory was not found in wheel." }

Remove-Item (Join-Path $targetDir "*") -Recurse -Force -ErrorAction SilentlyContinue
Copy-Item $exe.FullName (Join-Path $targetDir $exeName) -Force
Copy-Item $rg.FullName (Join-Path $targetDir $rg.Name) -Force
Copy-Item $office.FullName (Join-Path $targetDir $office.Name) -Recurse -Force

$sha = (Get-FileHash (Join-Path $targetDir $exeName) -Algorithm SHA256).Hash.ToLowerInvariant()
$manifest.sha256 = $sha
$manifest | ConvertTo-Json -Depth 8 | Set-Content $manifestPath -Encoding UTF8

Write-Host "Prepared pinned Harness runtime $version ($wheelVersion)"
Write-Host "SHA256 $sha"
