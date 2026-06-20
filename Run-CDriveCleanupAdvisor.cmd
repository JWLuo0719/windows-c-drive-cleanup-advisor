@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
set "REPORT_DIR=%SCRIPT_DIR%report"
powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%scripts\Scan-CDriveCleanupAdvisor.ps1" -Drive C -OutputDir "%REPORT_DIR%" -IncludeJson
echo.
echo Report folder:
echo %REPORT_DIR%
echo.
pause
