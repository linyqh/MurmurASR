# Builds the engine (if needed) and the Windows NSIS installer.
#
#   powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1
#   powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1 -Backend cpu -RebuildEngine
#
# Needs Visual Studio 2022 (C++ workload), Rust, Node 22, Git, and for the
# Vulkan backend a Vulkan SDK (VULKAN_SDK, or the newest C:\VulkanSDK\*).
param(
    [ValidateSet("vulkan", "cpu", "cuda")]
    [string]$Backend = "vulkan",
    [switch]$RebuildEngine
)
$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

$EngineDll = Join-Path $Root "src-tauri/resources/engine/lib/audiocpp.dll"
if ($RebuildEngine -or -not (Test-Path $EngineDll)) {
    if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
        $VsWhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio/Installer/vswhere.exe"
        $Vs = & $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        $VsCmake = Join-Path $Vs "Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin"
        if (-not (Test-Path (Join-Path $VsCmake "cmake.exe"))) { throw "CMake not found on PATH or in Visual Studio ($Vs)." }
        $env:PATH = "$VsCmake;$env:PATH"
    }
    if ($Backend -eq "vulkan") {
        if (-not $env:VULKAN_SDK) {
            $Sdk = Get-ChildItem C:\VulkanSDK -Directory -ErrorAction SilentlyContinue |
                Sort-Object Name -Descending | Select-Object -First 1
            if (-not $Sdk) { throw "Vulkan SDK not found. Install it from https://vulkan.lunarg.com or set VULKAN_SDK." }
            $env:VULKAN_SDK = $Sdk.FullName
        }
        $env:PATH = "$(Join-Path $env:VULKAN_SDK 'Bin');$env:PATH"
    }
    & (Join-Path $Root "engine/scripts/build-engine.ps1") -Backend $Backend
}

if (-not (Test-Path (Join-Path $Root "node_modules"))) {
    npm ci
    if ($LASTEXITCODE -ne 0) { throw "npm ci failed" }
}

npx tauri build
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

$Version = (Get-Content (Join-Path $Root "src-tauri/tauri.conf.json") -Raw | ConvertFrom-Json).version
$Installer = Join-Path $Root "src-tauri/target/release/bundle/nsis/Murmur_${Version}_x64-setup.exe"
if (-not (Test-Path $Installer)) { throw "Installer not found: $Installer" }
$Hash = (Get-FileHash -Algorithm SHA256 $Installer).Hash.ToLower()
"$Hash  $(Split-Path $Installer -Leaf)" | Set-Content -Encoding ascii "$Installer.sha256"
$Size = "{0:N1} MB" -f ((Get-Item $Installer).Length / 1MB)
Write-Host "Built $Installer ($Size)"
