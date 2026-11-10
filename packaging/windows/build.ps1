# Builds Windows packages (run on Windows with the MSVC Rust toolchain):
#   dist\Reckon-<v>-windows-x64.zip         portable archive (Reckon.exe app, bin\reckon.exe CLI, license notices)
#   dist\Reckon-<v>-windows-x64-setup.exe   installer (when Inno Setup's iscc is available)
#
#   pwsh packaging/windows/build.ps1
$ErrorActionPreference = "Stop"

$Root = Resolve-Path "$PSScriptRoot\..\.."
Set-Location $Root
$Version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"' | Select-Object -First 1).Matches.Groups[1].Value
$Out = Join-Path $Root "dist"
$Work = Join-Path $Root "target\package-windows"
$Stage = Join-Path $Work "Reckon"

cargo build --release -p reckon-app -p reckon-cli
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage, (Join-Path $Stage "bin"), $Out | Out-Null
# Windows file names are case-insensitive: "Reckon.exe" (app) and
# "reckon.exe" (CLI) cannot share a folder, so the CLI goes in bin\.
Copy-Item target\release\reckon-app.exe (Join-Path $Stage "Reckon.exe")
Copy-Item target\release\reckon.exe (Join-Path $Stage "bin\reckon.exe")
Copy-Item LICENSE (Join-Path $Stage "LICENSE.txt")
Copy-Item THIRD_PARTY_LICENSES.txt $Stage
Copy-Item README.md $Stage
Copy-Item packaging\windows\reckon.ico $Stage

$Zip = Join-Path $Out "Reckon-$Version-windows-x64.zip"
Remove-Item $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path "$Stage\*" -DestinationPath $Zip
Write-Output $Zip

$Iscc = Get-Command iscc -ErrorAction SilentlyContinue
if (-not $Iscc -and (Test-Path "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe")) {
    $Iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
}
if ($Iscc) {
    & $Iscc "/DAppVersion=$Version" "/DStageDir=$Stage" "/DOutDir=$Out" packaging\windows\reckon.iss
    if ($LASTEXITCODE -ne 0) { throw "iscc failed" }
    Write-Output (Join-Path $Out "Reckon-$Version-windows-x64-setup.exe")
} else {
    Write-Warning "Inno Setup not found; skipped the installer (zip only)."
}
