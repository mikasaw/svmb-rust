@echo off
rem svmb-rust build: cargo -> rename dll to sys -> test-sign (aligned with svmb/build/*.bat)
rem NOTE: keep this file ASCII-only (cmd reads it in ANSI codepage)
rem usage: build_rust.bat [Debug^|Release]   (default Debug)
rem output: %REPO%\x64\<config>\svmb_rust.sys + svmbctl-rs.exe
setlocal
set "REPO=%~dp0.."
set "CONFIG=%~1"
if "%CONFIG%"=="" set "CONFIG=Debug"
set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
rem bindgen in wdk-sys needs libclang (official CI uses LLVM 17; LLVM 23 breaks layout asserts)
rem WDK version auto-detected -> 10.0.28000 (the only WDK with km headers on this machine)
set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"

rem --- driver (standalone package per official windows-drivers-rs convention) ---
pushd "%REPO%\crates\svmb-driver"
if /i "%CONFIG%"=="Release" (set "CARGOARGS=--release") else (set "CARGOARGS=")
"%CARGO%" build %CARGOARGS%
if errorlevel 1 (popd & echo [build_rust] driver build FAILED & exit /b 1)
popd

if /i "%CONFIG%"=="Release" (set "SRC=%REPO%\crates\svmb-driver\target\release") else (set "SRC=%REPO%\crates\svmb-driver\target\debug")
set "OUTDIR=%REPO%\x64\%CONFIG%"
if not exist "%OUTDIR%" mkdir "%OUTDIR%"
copy /y "%SRC%\svmb_driver.dll" "%OUTDIR%\svmb_rust.sys" >nul
if errorlevel 1 (echo [build_rust] copy .sys FAILED & exit /b 1)

rem --- ctl (workspace member) ---
pushd "%REPO%"
"%CARGO%" build --package svmbctl-rs %CARGOARGS%
if errorlevel 1 (popd & echo [build_rust] ctl build FAILED & exit /b 1)
popd
if /i "%CONFIG%"=="Release" (set "SRC2=%REPO%\target\release") else (set "SRC2=%REPO%\target\debug")
copy /y "%SRC2%\svmbctl-rs.exe" "%OUTDIR%\svmbctl-rs.exe" >nul

rem --- test-sign (same as sign_sys_rel.bat: svmb-test cert in host 'My' store) ---
call "C:\Program Files\Microsoft Visual Studio\18\Insiders\VC\Auxiliary\Build\vcvars64.bat" >nul
signtool sign /fd sha256 /n svmb-test /s My "%OUTDIR%\svmb_rust.sys"
if errorlevel 1 (echo [build_rust] sign FAILED & exit /b 1)

echo [build_rust] OK: %OUTDIR%\svmb_rust.sys (+ svmbctl-rs.exe, config=%CONFIG%)
