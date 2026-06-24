@echo off
:: CSCEC Payment Voucher — Build & Install (Windows)
:: Double-click this file.

echo === CSCEC Payment Voucher — Build ^& Install ===
echo.

:: Admin check
net session >nul 2>&1
if %errorlevel% neq 0 (
    echo Requesting Administrator privileges...
    powershell -Command "Start-Process cmd -ArgumentList '/c \"%~f0\"' -Verb RunAs"
    exit /b 0
)

cd /d "%~dp0"
echo Running from: %CD%
echo.

:: Step 1: Rust
echo [1/8] Rust...
where rustc >nul 2>&1
if %errorlevel% neq 0 (
    echo   Downloading rustup-init.exe...
    curl.exe -L -o "%TEMP%\rustup-init.exe" "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe"
    start /wait "" "%TEMP%\rustup-init.exe" -y --default-toolchain stable --profile default
    setx /m PATH "%%PATH%%;%USERPROFILE%\.cargo\bin" >nul
    echo   Rust installed.
) else (
    rustc --version
)

:: Step 2: Node.js
echo.
echo [2/8] Node.js...
where node >nul 2>&1
if %errorlevel% neq 0 (
    winget install --id OpenJS.NodeJS --silent --accept-package-agreements --accept-source-agreements
) else (
    node --version
)

:: Step 3: C++ compiler (MinGW via MSYS2, ~50 MB — no Visual Studio needed)
echo.
echo [3/8] MinGW C++ compiler (for Rust GNU toolchain)...
where gcc >nul 2>&1
if %errorlevel% neq 0 (
    echo   Installing MSYS2 with MinGW...
    winget install --id MSYS2.MSYS2 --silent --accept-package-agreements --accept-source-agreements
    if exist "C:\msys64\usr\bin\bash.exe" (
        echo   Installing MinGW-w64 toolchain via pacman...
        C:\msys64\usr\bin\bash.exe -l -c "pacman -S --noconfirm mingw-w64-x86_64-gcc"
        setx /m PATH "%%PATH%%;C:\msys64\mingw64\bin" >nul
    )
    echo   MinGW installed.
) else (
    gcc --version | find "gcc" >nul && echo   gcc: found
)

:: Switch Rust to GNU toolchain if not already
echo.
echo   Configuring Rust GNU toolchain...
rustup toolchain list 2>nul | find "stable-gnu" >nul
if %errorlevel% neq 0 (
    rustup toolchain install stable-gnu
)
rustup default stable-gnu
rustc --version

:: Step 4: poppler
echo.
echo [4/8] poppler-utils...
where pdftotext >nul 2>&1
if %errorlevel% neq 0 (
    winget install --id oschwartz10612.poppler --silent --accept-package-agreements --accept-source-agreements
) else (
    echo   Already installed
)

:: Step 5: tesseract
echo.
echo [5/8] tesseract-OCR + chi_sim...
where tesseract >nul 2>&1
if %errorlevel% neq 0 (
    winget install --id UB-Mannheim.TesseractOCR --silent --accept-package-agreements --accept-source-agreements
)
if not exist "%ProgramFiles%\Tesseract-OCR\tessdata\chi_sim.traineddata" (
    curl.exe -L -o "%ProgramFiles%\Tesseract-OCR\tessdata\chi_sim.traineddata" "https://github.com/tesseract-ocr/tessdata/raw/main/chi_sim.traineddata"
)

:: Step 6: npm install
echo.
echo [6/8] npm install...
cd /d "%~dp0"
if not exist "package.json" (
    echo ERROR: package.json not found in %CD%
    pause
    exit /b 1
)
call npm install
if %errorlevel% neq 0 (
    echo npm install FAILED
    pause
    exit /b 1
)

:: Step 7: Build
echo.
echo [7/8] Building (first build: 5-15 minutes)...
rustup default stable-gnu
call npm run tauri build
if %errorlevel% neq 0 (
    echo Build FAILED
    pause
    exit /b 1
)

:: Step 8: Install
echo.
echo [8/8] Installing...
set "INSTALL_DIR=%LOCALAPPDATA%\CSCEC Payment Voucher"
if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"
if exist "dist\cscec-payment.exe" (
    copy /Y "dist\cscec-payment.exe" "%INSTALL_DIR%\cscec-payment.exe"
) else if exist "src-tauri\target\release\cscec-payment.exe" (
    copy /Y "src-tauri\target\release\cscec-payment.exe" "%INSTALL_DIR%\cscec-payment.exe"
)
setx /m PATH "%%PATH%%;%INSTALL_DIR%" >nul

echo   Creating desktop shortcut...
powershell -Command "$ws = New-Object -ComObject WScript.Shell; $s = $ws.CreateShortcut([Environment]::GetFolderPath('Desktop') + '\CSCEC Payment Voucher.lnk'); $s.TargetPath = '%INSTALL_DIR%\cscec-payment.exe'; $s.WorkingDirectory = '%INSTALL_DIR%'; $s.Save()"

echo.
echo ============================================
echo  ALL DONE!
echo ============================================
pause
