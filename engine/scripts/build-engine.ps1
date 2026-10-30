# Builds audiocpp.dll (audio.cpp C ABI) on Windows and stages it into
# src-tauri/resources/engine. Requires Visual Studio 2022 (C++), CMake and Git;
# the Vulkan backend also needs a Vulkan SDK (VULKAN_SDK).
#
# Builds the latest audio.cpp release tag (vX.Y.Z). Set $env:AUDIOCPP_REF to
# build a specific tag instead.
#
#   pwsh engine/scripts/build-engine.ps1                 # Vulkan (default)
#   pwsh engine/scripts/build-engine.ps1 -Backend cpu
#   pwsh engine/scripts/build-engine.ps1 -Backend cuda   # needs the CUDA toolkit
param(
    [ValidateSet("vulkan", "cpu", "cuda")]
    [string]$Backend = "vulkan",
    [switch]$Debug
)
$ErrorActionPreference = "Stop"

function Invoke-Native {
    & $args[0] @($args | Select-Object -Skip 1)
    if ($LASTEXITCODE -ne 0) { throw "$($args[0]) failed with exit code $LASTEXITCODE" }
}

$Root = Resolve-Path (Join-Path $PSScriptRoot "../..")
$Conf = @{}
Get-Content (Join-Path $Root "engine/audiocpp.conf") | ForEach-Object {
    if ($_ -match "^(\w+)=(.*)$") { $Conf[$Matches[1]] = $Matches[2] }
}

$Src = Join-Path $Root "engine/vendor/audio.cpp"
if (-not (Test-Path (Join-Path $Src ".git"))) {
    Invoke-Native git clone --filter=blob:none --no-checkout $Conf.AUDIOCPP_REPO $Src
}
$Ref = $env:AUDIOCPP_REF
if (-not $Ref) {
    $Ref = git ls-remote --tags --refs $Conf.AUDIOCPP_REPO "v*" |
        ForEach-Object { ($_ -split "refs/tags/")[1] } |
        Where-Object { $_ -match '^v\d+\.\d+\.\d+$' } |
        Sort-Object { [version]$_.TrimStart('v') } | Select-Object -Last 1
}
if ($Ref) {
    Invoke-Native git -C $Src fetch --depth 1 origin "refs/tags/${Ref}:refs/tags/${Ref}"
    Invoke-Native git -C $Src checkout --detach $Ref
} else {
    # Offline: rebuild whatever release is already checked out.
    $Ref = git -C $Src describe --tags --exact-match 2>$null
    if (-not $Ref) { throw "cannot reach $($Conf.AUDIOCPP_REPO) and no release is checked out" }
    Write-Warning "cannot reach $($Conf.AUDIOCPP_REPO); building the checked-out $Ref"
}
Write-Host "audio.cpp $Ref"

$BuildType = if ($Debug) { "RelWithDebInfo" } else { "Release" }
$Build = Join-Path $Root "engine/build/windows-$Backend"
$CmakeArgs = @(
    "-S", $Src, "-B", $Build, "-G", "Visual Studio 17 2022", "-A", "x64",
    "-DAUDIOCPP_BUILD_C_API=ON",
    "-DAUDIOCPP_DEPLOYMENT_BUILD=ON",
    "-DAUDIOCPP_MODEL_SET=custom",
    "-DAUDIOCPP_MODELS=$($Conf.AUDIOCPP_MODELS)",
    "-DAUDIOCPP_VERSION=$($Ref.TrimStart('v'))",
    "-DENGINE_ENABLE_NATIVE_CPU=OFF",
    "-DENGINE_ENABLE_METAL=OFF",
    "-DENGINE_ENABLE_HIP=OFF",
    "-DENGINE_ENABLE_VULKAN=$(if ($Backend -eq 'vulkan') { 'ON' } else { 'OFF' })",
    "-DENGINE_ENABLE_CUDA=$(if ($Backend -eq 'cuda') { 'ON' } else { 'OFF' })"
)
Invoke-Native cmake @CmakeArgs
# Upstream sets /utf-8 on only some targets; without it MSVC on non-UTF-8 code
# pages (e.g. GBK) misreads the UTF-8 Chinese literals in the model sources.
$env:CL = "/utf-8 $env:CL"
Invoke-Native cmake --build $Build --config $BuildType --parallel --target audiocpp

$Stage = Join-Path $Root "src-tauri/resources/engine"
$Lib = Join-Path $Stage "lib"
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $Lib, (Join-Path $Stage "assets") | Out-Null
Get-ChildItem (Join-Path $Build "bin/$BuildType") -Filter *.dll | Copy-Item -Destination $Lib
Copy-Item -Recurse (Join-Path $Src "assets/framework/models/silero_vad") (Join-Path $Stage "assets")
Copy-Item (Join-Path $Src "LICENSE") (Join-Path $Stage "LICENSE.audio.cpp")

# App-local runtime so the engine loads on machines without the VC++
# Redistributable or a Vulkan driver (the loader then reports no GPU and the
# engine falls back to the CPU).
$VsWhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio/Installer/vswhere.exe"
$Vs = & $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
$Redist = Get-ChildItem (Join-Path $Vs "VC/Redist/MSVC") -Directory |
    Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
    Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if (-not $Redist) { throw "MSVC redistributable files not found under $Vs" }
Get-ChildItem (Join-Path $Redist.FullName "x64") -Directory -Filter "Microsoft.VC*.CRT" |
    Get-ChildItem -Filter *.dll | Copy-Item -Destination $Lib
Get-ChildItem (Join-Path $Redist.FullName "x64") -Directory -Filter "Microsoft.VC*.OpenMP" |
    Get-ChildItem -Filter *.dll | Copy-Item -Destination $Lib

if ($Backend -eq "vulkan") {
    $Loader = Get-ChildItem $env:VULKAN_SDK -Recurse -Filter vulkan-1.dll -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -notmatch '\\(x86|Win32|32)\\' } | Select-Object -First 1
    if ($Loader) {
        Copy-Item $Loader.FullName $Lib
    } else {
        Write-Warning "vulkan-1.dll not found in VULKAN_SDK; the engine will require a system Vulkan loader."
    }
}

Write-Host "Engine staged at $Stage ($Backend)"
Get-ChildItem $Lib | Format-Table Name, Length -AutoSize
