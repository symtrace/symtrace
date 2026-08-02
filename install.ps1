# Symtrace Standalone Universal PowerShell Installer (Windows)
# Repository: https://github.com/JashT14/symtrace

$ErrorActionPreference = "Stop"

$Repo = "JashT14/symtrace"
$Target = "x86_64-pc-windows-msvc"
$ZipName = "symtrace-$Target.zip"
$DownloadUrl = "https://github.com/$Repo/releases/latest/download/$ZipName"
$InstallDir = Join-Path $env:LOCALAPPDATA "symtrace\bin"

function Show-Banner {
    Clear-Host
    Write-Host "  ____                  _                   " -ForegroundColor Cyan
    Write-Host " / ___| _7 _   _ _ __ ___ | |_ _ __ __ _  ___ ___ " -ForegroundColor Cyan
    Write-Host " \___ \| | | | | '_ \` _ \| __| '__/ _\` |/ __/ _ \" -ForegroundColor Cyan
    Write-Host "  ___) | |_| | | | | | | | |_| | | (_| | (_|  __/" -ForegroundColor Cyan
    Write-Host " |____/ \__, | |_| |_| |_|\__|_|  \__,_|\___\___|" -ForegroundColor Cyan
    Write-Host "        |___/                                    " -ForegroundColor Cyan
    Write-Host "   Deterministic AST Semantic Diff Engine v0.3.0" -ForegroundColor Magenta
    Write-Host "───────────────────────────────────────────────────" -ForegroundColor DarkGray
    Write-Host ""
}

function Show-SpinnerStep([string]$message) {
    $spinChars = @('⠋','⠙','⠹','⠸','⠼','⠴','⠦','⠧','⠇','⠏')
    for ($i = 0; $i -lt 10; $i++) {
        $char = $spinChars[$i % 8]
        Write-Host "`r[ " -NoNewline -ForegroundColor DarkGray
        Write-Host "$char" -NoNewline -ForegroundColor Cyan
        Write-Host " ] $message" -NoNewline -ForegroundColor White
        Start-Sleep -Milliseconds 60
    }
    Write-Host "`r[ " -NoNewline -ForegroundColor DarkGray
    Write-Host "✓" -NoNewline -ForegroundColor Green
    Write-Host " ] $message" -ForegroundColor White
}

Show-Banner

# ── Detect OS & Architecture ──────────────────────────────────────────
Show-SpinnerStep "Analyzing Windows system architecture ($Target)..."

$TmpDir = Join-Path $env:TEMP ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $TmpDir | Out-Null

try {
    # Download Release Archive
    Show-SpinnerStep "Downloading binary payload from GitHub Releases..."
    $ZipPath = Join-Path $TmpDir $ZipName
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath -UseBasicParsing

    # Extract Zip
    Show-SpinnerStep "Unpacking Tree-sitter grammars & AST engine..."
    Expand-Archive -Path $ZipPath -DestinationPath $TmpDir -Force

    # Install Binary
    Show-SpinnerStep "Binding 4-hash BLAKE3 identity tracker & LRU caches..."
    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    $ExePath = Join-Path $TmpDir "symtrace.exe"
    $DestPath = Join-Path $InstallDir "symtrace.exe"
    Copy-Item -Path $ExePath -Destination $DestPath -Force
    Start-Sleep -Milliseconds 100

    Write-Host ""
    Write-Host "✨ Installation Successful!" -ForegroundColor Green
    Write-Host "Binary Location : $DestPath" -ForegroundColor DarkGray

    # Update User PATH
    $UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        Write-Host "Updating User PATH environment variable..." -ForegroundColor Yellow
        $NewPath = "$UserPath;$InstallDir"
        [Environment]::SetEnvironmentVariable("PATH", $NewPath, "User")
        $env:PATH = "$env:PATH;$InstallDir"
    }

    Write-Host "Version Check   : "$NoNewline -ForegroundColor DarkGray
    Write-Host "$(& $DestPath --version)" -ForegroundColor Green
    Write-Host ""
    Write-Host "┌─► Symtrace is ready for semantic diffing! ⚡" -ForegroundColor Magenta
    Write-Host "└─► Try running: symtrace . HEAD~1 HEAD`n" -ForegroundColor DarkGray
}
finally {
    Remove-Item -Path $TmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
