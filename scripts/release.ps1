# EchoFlow release: build a signed installer + publish a GitHub Release so installed
# apps auto-update. Run AFTER bumping `version` in tauri.conf.json, Cargo.toml, and
# package.json to the same value.
#
#   powershell -File scripts\release.ps1 -Version 0.1.1 -Notes "What changed"
#
# Requires: the updater key in .keys\ (generated once with `tauri signer generate`)
# and `gh` authenticated with repo scope.

param(
  [Parameter(Mandatory = $true)][string]$Version,
  [string]$Notes = "Bug fixes and improvements.",
  [string]$Repo = "Mnourkh01/echoflow"
)
# NOTE: 'Continue', not 'Stop'. Native tools (npm/cargo/tauri/gh) print progress to
# stderr; under 'Stop' PowerShell 5.1 turns that into a terminating NativeCommandError
# and aborts before the build even starts. We gate on $LASTEXITCODE + explicit throws.
$ErrorActionPreference = "Continue"
$root = Split-Path -Parent $PSScriptRoot

# 1. Updater signing secrets (never printed, never committed).
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content "$root\.keys\echoflow_updater.key" -Raw).Trim()
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = (Get-Content "$root\.keys\echoflow_updater.password.txt" -Raw).Trim()

# 2. Build toolchain env + signed release bundle.
$env:Path = "C:\Users\mnour\.cargo\bin;" + $env:Path
$env:LIBCLANG_PATH = "C:\Users\mnour\AppData\Roaming\Python\Python314\site-packages\clang\native"
Set-Location $root
npm run tauri build
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

# 3. Locate the installer + its signature.
$nsisDir = "$root\src-tauri\target\release\bundle\nsis"
$setup = Get-ChildItem "$nsisDir\*-setup.exe" | Sort-Object LastWriteTime | Select-Object -Last 1
$sig = Get-ChildItem "$nsisDir\*-setup.exe.sig" | Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $setup -or -not $sig) { throw "installer or .sig not found in $nsisDir" }
$sigText = (Get-Content $sig.FullName -Raw).Trim()

# 4. Build latest.json (the manifest the app polls).
$url = "https://github.com/$Repo/releases/download/v$Version/$($setup.Name)"
$manifest = [ordered]@{
  version   = $Version
  notes     = $Notes
  pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  platforms = [ordered]@{ "windows-x86_64" = [ordered]@{ signature = $sigText; url = $url } }
}
# Write UTF-8 WITHOUT BOM: serde_json (Tauri updater) rejects a leading BOM.
[System.IO.File]::WriteAllText("$root\latest.json", ($manifest | ConvertTo-Json -Depth 6), (New-Object System.Text.UTF8Encoding $false))

# 5. Publish the release (installer + manifest). Endpoint points at /latest/download/latest.json.
gh release create "v$Version" $setup.FullName "$root\latest.json" --repo $Repo --title "EchoFlow v$Version" --notes $Notes
if ($LASTEXITCODE -ne 0) { throw "gh release create failed" }
Write-Output "Released v${Version}: $($setup.Name)"
