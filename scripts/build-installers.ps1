# Full release build: CPU (portable) + shareable CUDA (multi-arch, self-contained),
# named unambiguously, then mirrored into installers/ for local distribution.
# Version is read from package.json, so bumping the version is all you need.
#
#   CPU  -> Voxable_<ver>_x64_cpu-setup.exe  / _cpu_en-US.msi   (portable, ship this)
#   CUDA -> Voxable_<ver>_x64_cuda-setup.exe / _cuda_en-US.msi  (any NVIDIA machine)
#
# Run:  powershell -ExecutionPolicy Bypass -File scripts\build-installers.ps1
$ErrorActionPreference = "Continue"
$proj = "C:\LLM\kami-claude-vault\60-Experiments\omp\Projects\voxable"
$bundle = "$proj\target\release\bundle"
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:PATH = "C:\Users\hello\.cargo\bin;C:\Program Files\CMake\bin;$env:PATH"
Set-Location $proj
$ver = (Get-Content "$proj\package.json" -Raw | ConvertFrom-Json).version

# Purge stray CUDA runtime DLLs left next to the exe by a prior CUDA build — the
# WiX/MSI bundler harvests the binary dir, so leftover DLLs would bloat the CPU MSI.
Remove-Item "$proj\target\release\*.dll" -Force -ErrorAction SilentlyContinue

Write-Output "=== [1/2] CPU build v$ver ==="
npm run tauri build
if ($LASTEXITCODE -ne 0) { Write-Output "CPU BUILD FAILED ($LASTEXITCODE)"; exit 1 }
Copy-Item "$bundle\nsis\Voxable_${ver}_x64-setup.exe" "$bundle\nsis\Voxable_${ver}_x64_cpu-setup.exe" -Force
Copy-Item "$bundle\msi\Voxable_${ver}_x64_en-US.msi" "$bundle\msi\Voxable_${ver}_x64_cpu_en-US.msi" -Force
Remove-Item "$bundle\nsis\Voxable_${ver}_x64-setup.exe" -Force -ErrorAction SilentlyContinue
Remove-Item "$bundle\msi\Voxable_${ver}_x64_en-US.msi" -Force -ErrorAction SilentlyContinue

Write-Output "=== [2/2] Shareable CUDA build ==="
& "$PSScriptRoot\build-cuda-shareable.ps1"
if ($LASTEXITCODE -ne 0) { Write-Output "CUDA BUILD FAILED"; exit 2 }

Write-Output "=== Mirror into installers/ ==="
New-Item -ItemType Directory -Force "$proj\installers\windows-cpu","$proj\installers\windows-cuda" | Out-Null
Remove-Item "$proj\installers\windows-cpu\*","$proj\installers\windows-cuda\*" -Force -ErrorAction SilentlyContinue
Copy-Item "$bundle\nsis\Voxable_${ver}_x64_cpu-setup.exe","$bundle\msi\Voxable_${ver}_x64_cpu_en-US.msi" "$proj\installers\windows-cpu\" -Force
Copy-Item "$bundle\nsis\Voxable_${ver}_x64_cuda-setup.exe","$bundle\msi\Voxable_${ver}_x64_cuda_en-US.msi" "$proj\installers\windows-cuda\" -Force

Write-Output "=== DONE - final installers (v$ver) ==="
Get-ChildItem "$bundle\nsis\*.exe","$bundle\msi\*.msi" | Select-Object Name,Length | Format-Table -AutoSize | Out-String | Write-Output
Write-Output "ALL_DONE"
