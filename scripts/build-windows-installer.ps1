# OpenPTZ Windows Installer Build Script
# Builds both MSI (via WiX) and EXE Setup (via Inno Setup) if available.

param(
    [string]$Version = "0.1.0",
    [switch]$SkipBuild = $false
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectDir = Split-Path -Parent $ScriptDir
Set-Location $ProjectDir

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Building OpenPTZ Windows Distribution" -ForegroundColor Cyan
Write-Host "  Version: $Version" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# 1. Build release binary if needed
if (-not $SkipBuild) {
    Write-Host "[1/4] Compiling OpenPTZ in release mode..." -ForegroundColor Yellow
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Cargo build failed with exit code $LASTEXITCODE"
        exit $LASTEXITCODE
    }
} else {
    Write-Host "[1/4] Skipping binary compilation (--SkipBuild specified)..." -ForegroundColor Yellow
}

$ExePath = Join-Path $ProjectDir "target\release\openptz.exe"
if (-not (Test-Path $ExePath)) {
    Write-Error "Target executable not found at $ExePath"
    exit 1
}

# 2. Verify icon file
Write-Host "[2/4] Verifying Windows icon assets..." -ForegroundColor Yellow
$IconPath = Join-Path $ProjectDir "assets\openptz.ico"
if (-not (Test-Path $IconPath)) {
    Write-Host "Generating assets\openptz.ico from assets\openptz.svg..." -ForegroundColor DarkYellow
    $Magick = Get-Command magick.exe -ErrorAction SilentlyContinue
    if ($Magick) {
        & $Magick -background none assets\openptz.svg -define icon:auto-resize=256,128,64,48,32,16 assets\openptz.ico
    } else {
        Write-Warning "ImageMagick not found. Please ensure assets\openptz.ico exists."
    }
}

# Create output directory
$OutDir = Join-Path $ProjectDir "target\installer"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

# 3. Build WiX MSI Package if WiX / candle.exe is available
Write-Host "[3/4] Checking WiX Toolset for MSI generation..." -ForegroundColor Yellow
$Candle = Get-Command candle.exe -ErrorAction SilentlyContinue
$Light = Get-Command light.exe -ErrorAction SilentlyContinue

if (-not $Candle) {
    # Check standard install locations
    $WixBins = @(
        "C:\Program Files (x86)\WiX Toolset v3.11\bin\candle.exe",
        "C:\Program Files\WiX Toolset v3.11\bin\candle.exe"
    )
    foreach ($path in $WixBins) {
        if (Test-Path $path) {
            $Candle = $path
            $Light = $path.Replace("candle.exe", "light.exe")
            break
        }
    }
}

if ($Candle -and $Light) {
    Write-Host "Building MSI installer using WiX Toolset..." -ForegroundColor Green
    $WixOut = Join-Path $ProjectDir "target\wix"
    New-Item -ItemType Directory -Force -Path $WixOut | Out-Null
    
    $WixObj = Join-Path $WixOut "main.wixobj"
    $MsiFile = Join-Path $OutDir "openptz-windows-x86_64.msi"
    
    & $Candle -arch x64 -dVersion="$Version" -ext WixUIExtension (Join-Path $ProjectDir "wix\main.wxs") -out $WixObj
    if ($LASTEXITCODE -eq 0) {
        & $Light -ext WixUIExtension $WixObj -out $MsiFile
        if ($LASTEXITCODE -eq 0) {
            Write-Host "Successfully generated MSI: $MsiFile" -ForegroundColor Green
        } else {
            Write-Warning "WiX light.exe linker failed."
        }
    } else {
        Write-Warning "WiX candle.exe compiler failed."
    }
} else {
    Write-Host "WiX Toolset not found in PATH or standard directories. (Skipping MSI build)" -ForegroundColor DarkGray
    Write-Host "To enable MSI generation, install WiX v3.11: choco install wixtoolset" -ForegroundColor DarkGray
}

# 4. Build Inno Setup EXE if ISCC is available
Write-Host "[4/4] Checking Inno Setup for EXE wizard generation..." -ForegroundColor Yellow
$Iscc = Get-Command iscc.exe -ErrorAction SilentlyContinue
if (-not $Iscc) {
    $InnoPaths = @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe",
        "C:\Program Files (x86)\Inno Setup 5\ISCC.exe"
    )
    foreach ($path in $InnoPaths) {
        if (Test-Path $path) {
            $Iscc = $path
            break
        }
    }
}

if ($Iscc) {
    Write-Host "Building EXE installer using Inno Setup ($Iscc)..." -ForegroundColor Green
    $IssScript = Join-Path $ProjectDir "installer\windows\openptz.iss"
    & $Iscc $IssScript
    if ($LASTEXITCODE -eq 0) {
        Write-Host "Successfully generated Setup EXE in $OutDir" -ForegroundColor Green
    } else {
        Write-Warning "Inno Setup compilation failed."
    }
} else {
    Write-Host "Inno Setup not found in PATH or standard directories. (Skipping EXE wizard build)" -ForegroundColor DarkGray
    Write-Host "To enable EXE wizard generation, install Inno Setup: choco install innosetup" -ForegroundColor DarkGray
}

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Windows packaging process complete!" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
