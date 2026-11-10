@echo off
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0.."
title Stellars - Clean

rem Run from anywhere: it switches to the project root first.
rem Usage: .scripts\clean.bat [dry] [deep] [quiet]
rem   (no option)  removes regenerable build leftovers only; keeps the release compile cache,
rem                so the next build stays fast. The "release" folder with your installers is kept.
rem   dry          only lists what would be removed
rem   deep         also removes the whole Rust build folder (src-tauri\target) and node_modules;
rem                the next build starts from scratch and takes much longer
rem   quiet        no "press any key" at the end (used by build.bat)
rem Source code, settings, docs, design files and the installers are never touched.

set "DRY=0"
set "DEEP=0"
set "QUIET=0"
for %%A in (%*) do (
  if /i "%%~A"=="dry" set "DRY=1"
  if /i "%%~A"=="deep" set "DEEP=1"
  if /i "%%~A"=="quiet" set "QUIET=1"
)

echo.
if "%DRY%"=="1" (echo ==== Stellars clean ^(dry run - nothing is deleted^) ====) else (echo ==== Stellars clean ====)
echo.

set "FREED=0"

call :remove_dir "dist"                                  "front-end build output"
call :remove_dir "src-tauri\target\debug"                "debug build cache"
call :remove_dir "src-tauri\target\release\incremental"  "incremental compile data"
call :remove_dir "src-tauri\target\release\examples"     "example binaries"
call :remove_dir "src-tauri\target\release\bundle"       "installer working files - the finished installer is kept in release\"
call :remove_dir "src-tauri\target\tmp"                  "temporary build files"
call :remove_dir "src-tauri\gen"                         "generated Tauri schemas - recreated on the next build"

if "%DEEP%"=="1" (
  call :remove_dir "src-tauri\target"                    "entire Rust build folder"
  call :remove_dir "node_modules"                        "front-end packages - .scripts\build.bat reinstalls them"
)

rem Stray log and temp files at the top of the project and of src-tauri (tool folders are left alone).
for %%F in ("*.log" "*.tmp" "src-tauri\*.log" "src-tauri\*.tmp") do (
  if exist "%%~F" (
    if "%DRY%"=="1" (echo   would delete file: %%~F) else (del /f /q "%%~F" >nul 2>&1)
  )
)

echo.
if "%DRY%"=="1" (
  echo Dry run finished. Run .scripts\clean.bat without "dry" to remove these.
) else (
  echo Cleanup finished.
)
if "%QUIET%"=="0" pause
exit /b 0

:remove_dir
if not exist "%~1\" exit /b 0
if "%DRY%"=="1" (
  echo   would remove: %~1  - %~2
  exit /b 0
)
echo   removing: %~1  - %~2
rd /s /q "%~1" >nul 2>&1
if exist "%~1\" echo   [warning] could not remove everything in %~1 ^(a file may be in use^)
exit /b 0
