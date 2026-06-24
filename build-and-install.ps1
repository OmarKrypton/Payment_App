# CSCEC Payment Voucher — Build & Install (Windows)
# How to run:
#   1. Open PowerShell as Administrator
#   2. cd to the Payment_App folder
#   3. Paste this entire script

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $ScriptDir

Write-Host "=== CSCEC Payment Voucher — Build & Install ===" -ForegroundColor Cyan
Write-Host ""

# 1. Rust
Write-Host "[1/8] Rust..." -ForegroundColor Yellow
if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    Invoke-WebRequest -Uri "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe" -OutFile "$env:TEMP\rustup-init.exe"
    Start-Process -Wait "$env:TEMP\rustup-init.exe" -ArgumentList "-y", "--default-toolchain", "stable", "--profile", "default"
    $env:Path += ";$env:USERPROFILE\.cargo\bin"
    [Environment]::SetEnvironmentVariable("Path", [Environment]::GetEnvironmentVariable("Path","Machine") + ";$env:USERPROFILE\.cargo\bin", "Machine")
}
Write-Host "  $(rustc --version)" -ForegroundColor Green

# 2. Node.js
Write-Host ""
Write-Host "[2/8] Node.js..." -ForegroundColor Yellow
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    winget install --id OpenJS.NodeJS --silent --accept-package-agreements --accept-source-agreements
    $env:Path += ";${env:ProgramFiles}\nodejs"
}
Write-Host "  $(node --version)" -ForegroundColor Green

# 3. MSVC compiler (minimal — installs cl.exe, ~300 MB)
Write-Host ""
Write-Host "[3/8] MSVC C++ compiler..." -ForegroundColor Yellow
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
    Write-Host "  Downloading vs_BuildTools.exe..." -ForegroundColor Gray
    Invoke-WebRequest -Uri "https://aka.ms/vs/17/release/vs_BuildTools.exe" -OutFile "$env:TEMP\vs_BuildTools.exe"
    Write-Host "  Installing (quiet, ~3 min)..." -ForegroundColor Gray
    Start-Process -Wait -FilePath "$env:TEMP\vs_BuildTools.exe" -ArgumentList "--quiet", "--wait", "--norestart", "--add", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "--add", "Microsoft.VisualStudio.Component.Windows10SDK"
    Write-Host "  Done. Reboot may be needed." -ForegroundColor Yellow
} else {
    Write-Host "  Already installed" -ForegroundColor Green
}

# 4. poppler
Write-Host ""
Write-Host "[4/8] poppler-utils..." -ForegroundColor Yellow
if (-not (Get-Command pdftotext -ErrorAction SilentlyContinue)) {
    winget install --id oschwartz10612.poppler --silent --accept-package-agreements --accept-source-agreements
} else {
    Write-Host "  Already installed" -ForegroundColor Green
}

# 5. tesseract + chi_sim
Write-Host ""
Write-Host "[5/8] tesseract-OCR + chi_sim..." -ForegroundColor Yellow
if (-not (Get-Command tesseract -ErrorAction SilentlyContinue)) {
    winget install --id UB-Mannheim.TesseractOCR --silent --accept-package-agreements --accept-source-agreements
} else {
    Write-Host "  Already installed" -ForegroundColor Green
}
$tessdata = "${env:ProgramFiles}\Tesseract-OCR\tessdata"
if (-not (Test-Path "$tessdata\chi_sim.traineddata")) {
    Write-Host "  Downloading chi_sim.traineddata..." -ForegroundColor Gray
    Invoke-WebRequest -Uri "https://github.com/tesseract-ocr/tessdata/raw/main/chi_sim.traineddata" -OutFile "$tessdata\chi_sim.traineddata"
}

# 6. npm install
Write-Host ""
Write-Host "[6/8] npm install..." -ForegroundColor Yellow
if (-not (Test-Path "package.json")) { throw "package.json not found in $pwd" }
npm install
Write-Host "  OK" -ForegroundColor Green

# 7. Build
Write-Host ""
Write-Host "[7/8] Building (first build: 5-15 min)..." -ForegroundColor Yellow
npm run tauri build
Write-Host "  Build succeeded!" -ForegroundColor Green

# 8. Install
Write-Host ""
Write-Host "[8/8] Installing..." -ForegroundColor Yellow
$installDir = "$env:LOCALAPPDATA\CSCEC Payment Voucher"
New-Object -ItemType Directory -Force -Path $installDir | Out-Null
$src = if (Test-Path "dist\cscec-payment.exe") { "dist\cscec-payment.exe" } else { "src-tauri\target\release\cscec-payment.exe" }
Copy-Item $src -Destination "$installDir\cscec-payment.exe" -Force
$env:Path += ";$installDir"
[Environment]::SetEnvironmentVariable("Path", [Environment]::GetEnvironmentVariable("Path","Machine") + ";$installDir", "Machine")
$ws = New-Object -ComObject WScript.Shell
$s = $ws.CreateShortcut([Environment]::GetFolderPath("Desktop") + "\CSCEC Payment Voucher.lnk")
$s.TargetPath = "$installDir\cscec-payment.exe"
$s.WorkingDirectory = "$installDir"
$s.Save()

Write-Host ""
Write-Host "ALL DONE!" -ForegroundColor Cyan
Write-Host "Launch from desktop shortcut or run: cscec-payment.exe"
Read-Host "Press Enter to exit"
