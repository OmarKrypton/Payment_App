# CSCEC Payment Voucher — Installer (Windows)
# Run in PowerShell as Administrator
# Requires: winget (built into Windows 10 1709+ / Windows 11)

param(
    [switch]$NoDesktopShortcut = $false
)

$ErrorActionPreference = "Stop"
$AppName = "cscec-payment"
$InstallDir = "$env:LOCALAPPDATA\CSCEC Payment Voucher"
$BinarySource = ".\dist\$AppName.exe"
$BinaryTarget = "$InstallDir\$AppName.exe"

Write-Host "=== CSCEC Payment Voucher — Installer (Windows) ===" -ForegroundColor Cyan
Write-Host ""

# ── Install system dependencies via winget ─────────────────────
Write-Host ">>> Installing poppler and tesseract-ocr..." -ForegroundColor Yellow

$deps = @(
    @{ Name = "poppler"; Id = "oschwartz10612.poppler" },
    @{ Name = "tesseract"; Id = "UB-Mannheim.TesseractOCR" }
)

foreach ($dep in $deps) {
    $installed = winget list --id $dep.Id --accept-source-agreements 2>$null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  Installing $($dep.Name)..."
        winget install --id $dep.Id --silent --accept-package-agreements --accept-source-agreements
    } else {
        Write-Host "  $($dep.Name): already installed" -ForegroundColor Green
    }
}

# Add to machine PATH (for future shells)
$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
$popplerBin = "${env:ProgramFiles}\poppler\bin"
$tesseractDir = "${env:ProgramFiles}\Tesseract-OCR"

$pathsToAdd = @()
if ($machinePath -notlike "*$popplerBin*") { $pathsToAdd += $popplerBin }
if ($machinePath -notlike "*$tesseractDir*") { $pathsToAdd += $tesseractDir }

if ($pathsToAdd.Count -gt 0) {
    $newPath = $machinePath + ";" + ($pathsToAdd -join ";")
    [Environment]::SetEnvironmentVariable("Path", $newPath, "Machine")
    Write-Host "  Added to system PATH: $($pathsToAdd -join ', ')" -ForegroundColor Green
}
# Also update current session
$env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [Environment]::GetEnvironmentVariable("Path", "User")

# ── Download chi_sim traineddata ──────────────────────────────
Write-Host ""
Write-Host ">>> Ensuring chi_sim language data..." -ForegroundColor Yellow
$tessdataDir = "$tesseractDir\tessdata"
if (-not (Test-Path "$tessdataDir\chi_sim.traineddata")) {
    Write-Host "  Downloading chi_sim.traineddata (16 MB)..."
    $url = "https://github.com/tesseract-ocr/tessdata/raw/main/chi_sim.traineddata"
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor 3072
    Invoke-WebRequest -Uri $url -OutFile "$tessdataDir\chi_sim.traineddata"
    Write-Host "  Downloaded to $tessdataDir\chi_sim.traineddata" -ForegroundColor Green
} else {
    Write-Host "  chi_sim.traineddata: already present" -ForegroundColor Green
}

# Verify
Write-Host ""
Write-Host ">>> Verifying dependencies..." -ForegroundColor Yellow
$ok = $true
try {
    $v = & pdftotext --version 2>&1 | Select-Object -First 1
    Write-Host "  pdftotext: OK ($v)" -ForegroundColor Green
} catch {
    Write-Host "  WARNING: pdftotext not found in PATH" -ForegroundColor Red
    $ok = $false
}
try {
    $v = & tesseract --version 2>&1 | Select-Object -First 1
    Write-Host "  tesseract: OK ($v)" -ForegroundColor Green
    $langs = & tesseract --list-langs 2>&1
    if ($langs -match "chi_sim") {
        Write-Host "  chi_sim:   OK" -ForegroundColor Green
    } else {
        Write-Host "  WARNING: chi_sim not found in tesseract languages" -ForegroundColor Red
    }
} catch {
    Write-Host "  WARNING: tesseract not found in PATH" -ForegroundColor Red
    $ok = $false
}

# ── Install the app binary ───────────────────────────────────
Write-Host ""
Write-Host ">>> Installing $AppName..." -ForegroundColor Yellow
if (Test-Path $BinarySource) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item $BinarySource -Destination $BinaryTarget -Force
    Write-Host "  Installed: $BinarySource → $BinaryTarget" -ForegroundColor Green
} else {
    $releaseSource = ".\src-tauri\target\release\$AppName.exe"
    if (Test-Path $releaseSource) {
        New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
        Copy-Item $releaseSource -Destination $BinaryTarget -Force
        Write-Host "  Installed: $releaseSource → $BinaryTarget" -ForegroundColor Green
    } else {
        Write-Host "  WARNING: $AppName.exe binary not found." -ForegroundColor Red
        Write-Host "  Build it first: cd src-tauri && cargo build --release"
        Write-Host "  Then re-run this script."
    }
}

# Add app dir to PATH
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$InstallDir*") {
    $newUserPath = $userPath + ";$InstallDir"
    [Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")
    Write-Host "  Added to user PATH: $InstallDir" -ForegroundColor Green
}

# ── Desktop shortcut ─────────────────────────────────────────
if (-not $NoDesktopShortcut) {
    Write-Host ""
    Write-Host ">>> Creating desktop shortcut..." -ForegroundColor Yellow
    $desktop = [Environment]::GetFolderPath("Desktop")
    $wshell = New-Object -ComObject WScript.Shell
    $shortcut = $wshell.CreateShortcut("$desktop\CSCEC Payment Voucher.lnk")
    $shortcut.TargetPath = $BinaryTarget
    $shortcut.WorkingDirectory = $InstallDir
    $shortcut.Save()
    Write-Host "  Created: $desktop\CSCEC Payment Voucher.lnk" -ForegroundColor Green
}

Write-Host ""
Write-Host "=== Done! ===" -ForegroundColor Cyan
Write-Host "Launch from desktop shortcut or run: $AppName.exe"
if (-not $ok) {
    Write-Host ""
    Write-Host "NOTE: Some dependencies are missing. PDF import will not work until" -ForegroundColor Yellow
    Write-Host "      poppler-utils and tesseract-ocr are installed and in PATH."
}
