@echo off
setlocal EnableExtensions
cd /d "%~dp0.."
title Stellars - Build installer

rem Run from anywhere: it switches to the project root first.
rem Usage: .scripts\build.bat [noclean]
rem   Builds the NSIS installer (.exe) into the "release" folder, then runs clean.bat.
rem   "noclean" skips the automatic cleanup.

set "DO_CLEAN=1"
if /i "%~1"=="noclean" set "DO_CLEAN=0"

echo.
echo ==== Stellars installer build ====
echo.

where bun >nul 2>&1
if errorlevel 1 (
  echo [error] Bun was not found. Install it from https://bun.sh and try again.
  goto :fail
)
where cargo >nul 2>&1
if errorlevel 1 (
  echo [error] Rust ^(cargo^) was not found. Install it from https://rustup.rs and try again.
  goto :fail
)

if not exist "node_modules\" (
  echo Installing front-end packages...
  call bun install
  if errorlevel 1 goto :fail
)

echo Building the app and the installer. The first release build takes several minutes...
call bun run tauri build
if errorlevel 1 goto :fail

set "BUNDLE=src-tauri\target\release\bundle\nsis"
set "SETUP="
for /f "delims=" %%F in ('dir /b /o-d "%BUNDLE%\*-setup.exe" 2^>nul') do (
  if not defined SETUP set "SETUP=%%F"
)
if not defined SETUP (
  echo [error] The build finished but no installer was found in %BUNDLE%.
  goto :fail
)

if not exist "release\" mkdir "release"
copy /y "%BUNDLE%\%SETUP%" "release\%SETUP%" >nul
if errorlevel 1 (
  echo [error] Could not copy the installer to the release folder.
  goto :fail
)

for %%A in ("release\%SETUP%") do set "SIZE=%%~zA"
set /a SIZE_MB=%SIZE% / 1048576
echo.
echo Installer ready: release\%SETUP%  ^(about %SIZE_MB% MB^)

if "%DO_CLEAN%"=="1" (
  echo.
  echo Cleaning temporary build files...
  call "%~dp0clean.bat" quiet
)

echo.
echo Done.
if /i not "%~2"=="quiet" pause
exit /b 0

:fail
echo.
echo Build FAILED. Nothing was cleaned up, so you can inspect the output above.
pause
exit /b 1
