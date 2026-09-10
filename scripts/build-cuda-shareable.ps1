# Shareable CUDA build: multi-GPU-arch + self-contained (bundles the CUDA runtime
# DLLs next to the exe, so it runs on any Windows machine with an NVIDIA driver -
# no CUDA toolkit / PATH needed). Big installer (~400MB) because cublasLt is 463MB.
# Version is read from package.json so filenames track the real version.
$ErrorActionPreference = "Continue"
$proj = "C:\LLM\kami-claude-vault\60-Experiments\omp\Projects\voxable"
$bundle = "$proj\target\release\bundle"
$cuda = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.3"
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:CUDA_PATH = $cuda
$env:CUDA_PATH_V13_3 = $cuda
# Turing(75) Ampere(86) Ada(89) Blackwell(120) - covers 20xx..50xx consumer GPUs.
$env:CUDAARCHS = "75;86;89;120"
$env:PATH = "C:\Users\hello\.cargo\bin;C:\Program Files\CMake\bin;$cuda\bin;$env:PATH"
Set-Location $proj
$ver = (Get-Content "$proj\package.json" -Raw | ConvertFrom-Json).version

Write-Output "=== Shareable CUDA build v$ver (archs $env:CUDAARCHS) ==="
npm run tauri build -- --features cuda --config src-tauri/tauri.cuda.conf.json
if ($LASTEXITCODE -ne 0) { Write-Output "CUDA BUILD FAILED ($LASTEXITCODE)"; exit 1 }

Copy-Item "$bundle\nsis\Voxable_${ver}_x64-setup.exe" "$bundle\nsis\Voxable_${ver}_x64_cuda-setup.exe" -Force
Copy-Item "$bundle\msi\Voxable_${ver}_x64_en-US.msi" "$bundle\msi\Voxable_${ver}_x64_cuda_en-US.msi" -Force
Remove-Item "$bundle\nsis\Voxable_${ver}_x64-setup.exe" -Force -ErrorAction SilentlyContinue
Remove-Item "$bundle\msi\Voxable_${ver}_x64_en-US.msi" -Force -ErrorAction SilentlyContinue

Write-Output "=== DLLs bundled next to target/release exe? ==="
Get-ChildItem "$proj\target\release\*.dll" | Select-Object Name,Length | Format-Table -AutoSize | Out-String | Write-Output
Write-Output "ALL_DONE"
