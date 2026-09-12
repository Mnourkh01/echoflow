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

# 2. Build toolchain env + signed release bundle. Machine-specific native-build
#    paths (see the windows-native-build notes): portable CMake + libclang from
#    .tools, with the pip-wheel libclang as a fallback if .tools was wiped. The
#    CMAKE_GENERATOR pin is required — the cmake crate otherwise auto-picks
#    "Visual Studio 18 2026", which portable CMake 3.30.5 doesn't know and aborts.
$cmakeBin = "$root\.tools\cmake-3.30.5-windows-x86_64\bin"
$env:Path = "C:\Users\mnour\.cargo\bin;$cmakeBin;" + $env:Path
$libclangTools = "$root\.tools\libclang"
$libclangPip = "C:\Users\mnour\AppData\Roaming\Python\Python314\site-packages\clang\native"
$env:LIBCLANG_PATH = if (Test-Path "$libclangTools\libclang.dll") { $libclangTools } else { $libclangPip }
$env:CMAKE_GENERATOR = "Visual Studio 16 2019"
$env:CMAKE_GENERATOR_PLATFORM = "x64"
# Vulkan GPU build (whisper.cpp ggml-vulkan). The "mini SDK" is assembled by
# hand (Khronos headers + import lib generated from System32\vulkan-1.dll +
# glslc from Google's shaderc artifacts) because the LunarG installer refuses
# to run unattended without elevation. See the windows-native-build notes.
$env:VULKAN_SDK = "C:\VulkanSDK\mini"
$env:Path = "$env:VULKAN_SDK\Bin;" + $env:Path
Set-Location $root
npm run tauri build -- --features gpu-vulkan
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

# 3. Locate the installer + its signature. Ask cargo for the target dir: a machine-wide
#    target-dir in ~/.cargo/config.toml moves the bundle out of src-tauri\target, and
#    that shared dir can hold older installers, so match this exact version.
$targetDir = (cargo metadata --no-deps --format-version 1 --manifest-path "$root\src-tauri\Cargo.toml" | ConvertFrom-Json).target_directory
$nsisDir = "$targetDir\release\bundle\nsis"
$setup = Get-ChildItem "$nsisDir\*_${Version}_*-setup.exe" | Sort-Object LastWriteTime | Select-Object -Last 1
$sig = Get-ChildItem "$nsisDir\*_${Version}_*-setup.exe.sig" | Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $setup -or -not $sig) { throw "installer or .sig for $Version not found in $nsisDir" }
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
