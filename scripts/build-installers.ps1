# Build both installers (CPU + CUDA) and name them unambiguously.
#   CPU  -> Voxable_..._cpu-setup.exe  / _cpu_en-US.msi   (portable, ship this)
#   CUDA -> Voxable_..._cuda-setup.exe / _cuda_en-US.msi  (this machine only)
#
# Run from anywhere:  powershell -ExecutionPolicy Bypass -File scripts\build-installers.ps1
# Requires cargo + cmake + LLVM + CUDA toolkit installed (see BUILD.md).

$ErrorActionPreference = "Continue"
$proj = Split-Path -Parent $PSScriptRoot
$bundle = "$proj\src-tauri\target\release\bundle"
if (-not (Test-Path $bundle)) { $bundle = "$proj\target\release\bundle" }

$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:PATH = "C:\Users\hello\.cargo\bin;C:\Program Files\CMake\bin;$env:PATH"
Set-Location $proj

Write-Output "=== [1/2] CPU build ==="
npm run tauri build
if ($LASTEXITCODE -ne 0) { Write-Output "CPU BUILD FAILED ($LASTEXITCODE)"; exit 1 }
Copy-Item "$bundle\nsis\Voxable_0.1.0_x64-setup.exe" "$bundle\nsis\Voxable_0.1.0_x64_cpu-setup.exe" -Force
Copy-Item "$bundle\msi\Voxable_0.1.0_x64_en-US.msi" "$bundle\msi\Voxable_0.1.0_x64_cpu_en-US.msi" -Force

Write-Output "=== [2/2] CUDA build ==="
$cuda = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.3"
$env:CUDA_PATH = $cuda
$env:CUDA_PATH_V13_3 = $cuda
$env:CUDAARCHS = "native"
$env:PATH = "$cuda\bin;$env:PATH"
npm run tauri build -- --features cuda
if ($LASTEXITCODE -ne 0) { Write-Output "CUDA BUILD FAILED ($LASTEXITCODE)"; exit 2 }
Copy-Item "$bundle\nsis\Voxable_0.1.0_x64-setup.exe" "$bundle\nsis\Voxable_0.1.0_x64_cuda-setup.exe" -Force
Copy-Item "$bundle\msi\Voxable_0.1.0_x64_en-US.msi" "$bundle\msi\Voxable_0.1.0_x64_cuda_en-US.msi" -Force

# Drop the ambiguous generic-named files.
Remove-Item "$bundle\nsis\Voxable_0.1.0_x64-setup.exe" -Force -ErrorAction SilentlyContinue
Remove-Item "$bundle\msi\Voxable_0.1.0_x64_en-US.msi" -Force -ErrorAction SilentlyContinue

Write-Output "=== DONE - final artifacts ==="
Get-ChildItem "$bundle\nsis\*.exe","$bundle\msi\*.msi" | Select-Object Name,Length | Format-Table -AutoSize | Out-String | Write-Output
