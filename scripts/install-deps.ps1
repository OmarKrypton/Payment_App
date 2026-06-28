# CSCEC Payment Voucher — Install Dependencies (Windows)
# Run in PowerShell as Administrator
# Requires: winget (built into Windows 10 1709+ / Windows 11)

$ErrorActionPreference = "Stop"

Write-Host "=== Installing dependencies: poppler, tesseract-ocr, ImageMagick ===" -ForegroundColor Cyan
Write-Host ""

# ── Helper: check if winget package is installed ──────────────
function Is-Installed($id) {
    $r = winget list --id $id --accept-source-agreements 2>$null
    return $LASTEXITCODE -eq 0
}

# ── Install poppler via winget ─────────────────────────────────
Write-Host ">>> poppler (pdftotext, pdftoppm)..." -ForegroundColor Yellow
$popplerId = "oschwartz10612.poppler"
if (Is-Installed $popplerId) {
    Write-Host "  already installed" -ForegroundColor Green
} else {
    Write-Host "  Installing..."
    winget install --id $popplerId --silent --accept-package-agreements --accept-source-agreements
    Write-Host "  Done" -ForegroundColor Green
}
Write-Host ""

# ── Install tesseract-ocr via winget ──────────────────────────
Write-Host ">>> tesseract-ocr..." -ForegroundColor Yellow
$tesseractId = "UB-Mannheim.TesseractOCR"
$tesseractDir = "${env:ProgramFiles}\Tesseract-OCR"
$tessdataDir = "$tesseractDir\tessdata"

if (Is-Installed $tesseractId) {
    Write-Host "  already installed" -ForegroundColor Green
} else {
    Write-Host "  Installing..."
    winget install --id $tesseractId --silent --accept-package-agreements --accept-source-agreements

    # Refresh PATH to find tesseract in this session
    $env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [Environment]::GetEnvironmentVariable("Path", "User")
    Write-Host "  Done" -ForegroundColor Green
}

# ── Download chi_sim.traineddata ──────────────────────────────
Write-Host ">>> Chinese language data for tesseract..." -ForegroundColor Yellow
if (-not (Test-Path "$tessdataDir\chi_sim.traineddata")) {
    Write-Host "  Downloading chi_sim.traineddata (16 MB)..."
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor 3072
    Invoke-WebRequest -Uri "https://github.com/tesseract-ocr/tessdata/raw/main/chi_sim.traineddata" `
                      -OutFile "$tessdataDir\chi_sim.traineddata"
    Write-Host "  Downloaded to $tessdataDir\chi_sim.traineddata" -ForegroundColor Green
} else {
    Write-Host "  already present" -ForegroundColor Green
}
Write-Host ""

# ── Install ImageMagick (optional, for image preprocessing) ──
Write-Host ">>> ImageMagick (optional, for image preprocessing)..." -ForegroundColor Yellow
$magickId = "ImageMagick.ImageMagick"
if (Is-Installed $magickId) {
    Write-Host "  already installed" -ForegroundColor Green
} else {
    Write-Host "  Installing..."
    winget install --id $magickId --silent --accept-package-agreements --accept-source-agreements
    Write-Host "  Done" -ForegroundColor Green
}
Write-Host ""

# ── Add tesseract to system PATH (poppler found via winget cache, no PATH needed) ──
$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
if ($machinePath -notlike "*$tesseractDir*") {
    $newPath = $machinePath + ";$tesseractDir"
    [Environment]::SetEnvironmentVariable("Path", $newPath, "Machine")
    Write-Host "Added tesseract to system PATH" -ForegroundColor Green
}
# Refresh PATH for verification
$env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [Environment]::GetEnvironmentVariable("Path", "User")

# ── Verify ─────────────────────────────────────────────────────
Write-Host ">>> Verifying..." -ForegroundColor Yellow
$allOk = $true

try {
    $v = & tesseract --version 2>&1 | Select-Object -First 1
    Write-Host "  tesseract: $v" -ForegroundColor Green
    $langs = & tesseract --list-langs 2>&1
    if ($langs -match "chi_sim") {
        Write-Host "  chi_sim:   available" -ForegroundColor Green
    } else {
        Write-Host "  WARNING: chi_sim not found. PDF import will use English OCR only." -ForegroundColor Yellow
    }
} catch {
    Write-Host "  ERROR: tesseract not found. Restart terminal or re-run script as Administrator." -ForegroundColor Red
    $allOk = $false
}

# Test poppler via winget cache (where the app will find it)
$localAppData = $env:LOCALAPPDATA
$wingetRoot = "$localAppData\Microsoft\WinGet\Packages"
$popplerDir = Get-ChildItem "$wingetRoot\*poppler*" -Directory -ErrorAction SilentlyContinue | Select-Object -First 1
if ($popplerDir) {
    $pdftotextExe = Get-ChildItem $popplerDir.FullName -Recurse -Filter "pdftotext.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($pdftotextExe) {
        Write-Host "  pdftotext: $($pdftotextExe.FullName)" -ForegroundColor Green
    } else {
        Write-Host "  WARNING: pdftotext.exe not found in winget poppler cache" -ForegroundColor Yellow
    }
    $pdftoppmExe = Get-ChildItem $popplerDir.FullName -Recurse -Filter "pdftoppm.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($pdftoppmExe) {
        Write-Host "  pdftoppm:  $($pdftoppmExe.FullName)" -ForegroundColor Green
    }
} else {
    # Try PATH
    try {
        Write-Host "  pdftotext: found on PATH" -ForegroundColor Green
    } catch {
        Write-Host "  ERROR: poppler not found. If installation succeeded, restart terminal." -ForegroundColor Red
        $allOk = $false
    }
}

# ImageMagick check
try {
    Write-Host "  ImageMagick: found" -ForegroundColor Green
} catch {
    Write-Host "  ImageMagick: not found (optional - preprocessing will be skipped)" -ForegroundColor Yellow
}

Write-Host ""
if ($allOk) {
    Write-Host "=== All dependencies installed successfully! ===" -ForegroundColor Cyan
    Write-Host "Launch the app and import a PDF to verify."
} else {
    Write-Host "=== Some dependencies have issues (see WARNING/ERROR above) ===" -ForegroundColor Yellow
    Write-Host "Restart your terminal and re-run this script as Administrator."
}
