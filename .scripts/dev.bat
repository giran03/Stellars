@echo off
setlocal
title Stellars Sonar - Dev Mode

echo ============================================================
echo   Starting Stellars Sonar in Development Mode
echo ============================================================
echo.

:: Navigate to project root directory
cd /d "%~dp0.."

:: Verify bun availability
where bun >nul 2>&1
if %ERRORLEVEL% EQU 0 (
    echo [INFO] Starting Tauri v2 dev app with Bun...
    call bun run tauri dev
    goto :done
)

:: Fallback to npm
where npm >nul 2>&1
if %ERRORLEVEL% EQU 0 (
    echo [WARN] Bun not found in PATH, falling back to npm...
    call npm run tauri dev
    goto :done
)

:: Fallback to cargo tauri
where cargo >nul 2>&1
if %ERRORLEVEL% EQU 0 (
    echo [WARN] Bun and npm not found, attempting cargo tauri dev...
    call cargo tauri dev
    goto :done
)

echo.
echo [ERROR] Neither Bun, npm, nor cargo was found in PATH.
echo Please install Bun (https://bun.sh) or Node.js.
pause
exit /b 1

:done
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [ERROR] Dev process exited with error code %ERRORLEVEL%.
    pause
)
