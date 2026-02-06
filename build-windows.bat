@echo off
REM Windows packaged release build for lancer-apktool.
REM Output: dist\lancer-apktool-windows-x86_64\bin\lancer-apktool.exe

echo === Building lancer-apktool (release) for Windows x86_64 ===
cargo build --release
if errorlevel 1 exit /b 1

echo === Packaging ===
set DIST=dist\lancer-apktool-windows-x86_64
if exist "%DIST%" rmdir /s /q "%DIST%"
mkdir "%DIST%"
mkdir "%DIST%\bin"

copy target\release\lancer-apktool.exe "%DIST%\bin\" >nul
copy target\release\lancer-apktool-cli.exe "%DIST%\bin\" >nul 2>nul
copy LICENSE "%DIST%\" >nul 2>nul
copy README.md "%DIST%\" >nul 2>nul
if exist frameworks xcopy /e /i /y frameworks "%DIST%\frameworks" >nul 2>nul
if exist keystore xcopy /e /i /y keystore "%DIST%\keystore" >nul 2>nul

echo === Done. Binary: %DIST%\bin ===
dir "%DIST%\bin"
exit /b 0
