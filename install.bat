@echo off
REM CSCEC Payment Voucher — Installer (Windows)
REM Double-click this file or run from CMD

echo === CSCEC Payment Voucher — Installer (Windows) ===
echo.

echo Checking for winget...
where winget >nul 2>nul
if %ERRORLEVEL% neq 0 (
    echo ERROR: winget not found. Install the App Installer from the Microsoft Store.
    pause
    exit /b 1
)

echo.
echo Step 1: Install poppler-utils via winget...
winget install --id oschwartz10612.poppler --silent --accept-package-agreements --accept-source-agreements 2>nul
if %ERRORLEVEL% equ 0 (
    echo   poppler installed.
) else (
    echo   poppler may already be installed (or install failed — check manually).
)

echo.
echo Step 2: Install tesseract-OCR via winget...
winget install --id UB-Mannheim.TesseractOCR --silent --accept-package-agreements --accept-source-agreements 2>nul
if %ERRORLEVEL% equ 0 (
    echo   tesseract installed.
) else (
    echo   tesseract may already be installed (or install failed — check manually).
)

echo.
echo Step 3: Download chi_sim language data...
set "TESSDATA=%ProgramFiles%\Tesseract-OCR\tessdata"
if not exist "%TESSDATA%\chi_sim.traineddata" (
    echo   Downloading chi_sim.traineddata (16 MB)...
    powershell -Command "[Net.ServicePointManager]::SecurityProtocol = 3072; Invoke-WebRequest -Uri 'https://github.com/tesseract-ocr/tessdata/raw/main/chi_sim.traineddata' -OutFile '%TESSDATA%\chi_sim.traineddata'"
    if exist "%TESSDATA%\chi_sim.traineddata" (
        echo   Downloaded to %TESSDATA%\chi_sim.traineddata
    ) else (
        echo   Download failed. Run install.ps1 as Administrator for full setup.
    )
) else (
    echo   chi_sim.traineddata already present.
)

echo.
echo Step 4: Install cscec-payment binary...
set "INSTALL_DIR=%LOCALAPPDATA%\CSCEC Payment Voucher"
if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"

if exist "dist\cscec-payment.exe" (
    copy /Y "dist\cscec-payment.exe" "%INSTALL_DIR%\cscec-payment.exe"
    echo   Installed: dist\cscec-payment.exe
) else if exist "src-tauri\target\release\cscec-payment.exe" (
    copy /Y "src-tauri\target\release\cscec-payment.exe" "%INSTALL_DIR%\cscec-payment.exe"
    echo   Installed: src-tauri\target\release\cscec-payment.exe
) else (
    echo   WARNING: cscec-payment.exe not found.
    echo   Build it first: cd src-tauri ^&^& cargo build --release
)

echo.
echo === Done! ===
echo Launch from: %INSTALL_DIR%\cscec-payment.exe
pause
