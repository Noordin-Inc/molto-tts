[CmdletBinding()]
param(
    [switch]$Release,
    [switch]$Register
)

$ErrorActionPreference = "Stop"

$profile = if ($Release) { "release" } else { "debug" }
$flag = if ($Release) { "--release" } else { "" }

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Building Molto-TTS ($profile)         " -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# Safe rename of potentially locked binaries before cargo build
$binDir = "target\$profile"
foreach ($b in @("molto-tts.exe", "molto_tts_engine.dll", "molto_tts_engine_x86.dll")) {
    $p = "$binDir\$b"
    if (Test-Path $p) {
        $old = "$p.old"
        if (Test-Path $old) { Remove-Item $old -Force -ErrorAction SilentlyContinue }
        try {
            # Try a quick touch to see if it's locked
            [System.IO.File]::OpenWrite($p).Close()
        } catch {
            Move-Item -Path $p -Destination $old -Force -ErrorAction SilentlyContinue
        }
    }
}

# 1. Build 64-bit (GUI + Engine)
Write-Host "`n[1/3] Building 64-bit (x86_64)..." -ForegroundColor Yellow
if ($Release) {
    cargo build --workspace --release
} else {
    cargo build --workspace
}

# 2. Check if i686 target is installed, and build 32-bit engine
Write-Host "`n[2/3] Checking 32-bit (i686) target..." -ForegroundColor Yellow
$targets = rustup target list --installed
if ($targets -contains "i686-pc-windows-msvc") {
    Write-Host "Building 32-bit engine..." -ForegroundColor Green
    if ($Release) {
        cargo build -p molto-engine --target i686-pc-windows-msvc --release
    } else {
        cargo build -p molto-engine --target i686-pc-windows-msvc
    }

    # Automatically copy 32-bit DLL with distinct _x86 suffix next to 64-bit binary
    $src32 = "target\i686-pc-windows-msvc\$profile\molto_tts_engine.dll"
    $dst32 = "target\$profile\molto_tts_engine_x86.dll"
    if (Test-Path $src32) {
        try {
            Copy-Item -Path $src32 -Destination $dst32 -Force
        } catch {
            $old = "$dst32.old"
            if (Test-Path $old) { Remove-Item $old -Force -ErrorAction SilentlyContinue }
            Move-Item -Path $dst32 -Destination $old -Force
            Copy-Item -Path $src32 -Destination $dst32 -Force
        }
        Write-Host "Synchronized 32-bit engine -> $dst32" -ForegroundColor Green
    }
} else {
    Write-Host "Notice: i686-pc-windows-msvc not installed. Run 'rustup target add i686-pc-windows-msvc' to build 32-bit engine." -ForegroundColor Yellow
}

Write-Host "`n[3/3] Build outputs in target\$profile\:" -ForegroundColor Cyan
Get-ChildItem "target\$profile" -Include "molto-tts.exe","molto_tts_engine.dll","molto_tts_engine_x86.dll" | Select-Object Name, Length, LastWriteTime | Format-Table -AutoSize

if ($Register) {
    Write-Host "`nRegistering SAPI voices system-wide (Administrator)..." -ForegroundColor Yellow
    & "target\$profile\molto-tts.exe" register-all
}
