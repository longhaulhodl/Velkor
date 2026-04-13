@echo off
:: =============================================================================
:: Velkor — Windows CMD installer
::
:: Usage (from CMD):
::   curl -fsSL https://raw.githubusercontent.com/longhaulhodl/Velkor/main/scripts/install.bat -o install.bat && install.bat
:: =============================================================================

echo.
echo  Launching Velkor installer via PowerShell...
echo.

powershell -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/longhaulhodl/Velkor/main/scripts/install.ps1 | iex"

if %ERRORLEVEL% NEQ 0 (
    echo.
    echo  Installation failed. Make sure PowerShell is available.
    echo.
    pause
)
