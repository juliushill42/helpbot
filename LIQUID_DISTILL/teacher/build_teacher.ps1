Param(
    [string]$ModelPath = "MiniMax-M2.1.q2_k.gguf"
)

# Windows build helper for serve_teacher using CMake/MSVC.
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $root

if (-not (Test-Path ".\llama.cpp")) {
    git clone https://github.com/ggerganov/llama.cpp.git
}

# Build llama.cpp (Release)
cmake -S .\llama.cpp -B .\llama.cpp\build -DCMAKE_BUILD_TYPE=Release -DLLAMA_BUILD_EXAMPLES=OFF
cmake --build .\llama.cpp\build --config Release

# Build serve_teacher
if (-not (Test-Path ".\build")) { New-Item -ItemType Directory -Path .\build | Out-Null }
$serveSrc = Join-Path $root "teacher\serve_teacher.cpp"
$llamaInc = Join-Path $root "llama.cpp"
$llamaLibDir = Join-Path $root "llama.cpp\build\bin\Release"
$outExe = Join-Path $root "teacher\serve_teacher.exe"

cl /O2 /std:c++17 /EHsc /I"$llamaInc" $serveSrc /Fe"$outExe" /link /LIBPATH:"$llamaLibDir" llama.lib

Write-Host "Build complete: $outExe"

Pop-Location
