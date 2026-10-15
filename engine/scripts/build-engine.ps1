# Builds audiocpp.dll (audio.cpp C ABI) on Windows and stages it into
# src-tauri/resources/engine. Requires Visual Studio 2022 (C++), CMake and Git.
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

$Root = Resolve-Path (Join-Path $PSScriptRoot "../..")
$Lock = @{}
Get-Content (Join-Path $Root "engine/audiocpp.lock") | ForEach-Object {
    if ($_ -match "^(\w+)=(.*)$") { $Lock[$Matches[1]] = $Matches[2] }
}

$Src = Join-Path $Root "engine/vendor/audio.cpp"
if (-not (Test-Path (Join-Path $Src ".git"))) {
    git clone --filter=blob:none --no-checkout $Lock.AUDIOCPP_REPO $Src
}
$Head = (git -C $Src rev-parse HEAD 2>$null)
if ($Head -ne $Lock.AUDIOCPP_COMMIT) {
    git -C $Src fetch --depth 1 origin $Lock.AUDIOCPP_COMMIT
    git -C $Src checkout --detach $Lock.AUDIOCPP_COMMIT
}

$BuildType = if ($Debug) { "RelWithDebInfo" } else { "Release" }
$Build = Join-Path $Root "engine/build/windows-$Backend"
$Args = @(
    "-S", $Src, "-B", $Build, "-G", "Visual Studio 17 2022", "-A", "x64",
    "-DAUDIOCPP_BUILD_C_API=ON",
    "-DAUDIOCPP_DEPLOYMENT_BUILD=ON",
    "-DAUDIOCPP_MODEL_SET=custom",
    "-DAUDIOCPP_MODELS=$($Lock.AUDIOCPP_MODELS)",
    "-DAUDIOCPP_VERSION=$($Lock.AUDIOCPP_REF.TrimStart('v'))",
    "-DENGINE_ENABLE_NATIVE_CPU=OFF",
    "-DENGINE_ENABLE_METAL=OFF",
    "-DENGINE_ENABLE_HIP=OFF",
    "-DENGINE_ENABLE_VULKAN=$(if ($Backend -eq 'vulkan') { 'ON' } else { 'OFF' })",
    "-DENGINE_ENABLE_CUDA=$(if ($Backend -eq 'cuda') { 'ON' } else { 'OFF' })"
)
cmake @Args
cmake --build $Build --config $BuildType --parallel --target audiocpp

$Stage = Join-Path $Root "src-tauri/resources/engine"
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force (Join-Path $Stage "lib"), (Join-Path $Stage "assets") | Out-Null
Get-ChildItem (Join-Path $Build "bin/$BuildType") -Filter *.dll | Copy-Item -Destination (Join-Path $Stage "lib")
Copy-Item -Recurse (Join-Path $Src "assets/framework/models/silero_vad") (Join-Path $Stage "assets")
Copy-Item (Join-Path $Src "LICENSE") (Join-Path $Stage "LICENSE.audio.cpp")
Write-Host "Engine staged at $Stage ($Backend)"
