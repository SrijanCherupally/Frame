param(
    [string]$FfmpegDir,
    [string]$FfmpegLicenseDir,
    [string]$OutputDir = "windows-package"
)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)
if ($env:OS -ne "Windows_NT") { throw "Run this script on Windows with Rust and Visual Studio C++ Build Tools installed." }
$packagePath = Join-Path (Get-Location) $OutputDir
if (Test-Path $packagePath) { throw "Output folder already exists. Choose a new -OutputDir to preserve previous packages." }
if ($FfmpegDir) {
    foreach ($name in @("ffmpeg.exe", "ffprobe.exe")) {
        if (!(Test-Path (Join-Path $FfmpegDir $name))) { throw "Missing $name in -FfmpegDir." }
    }
    if (!$FfmpegLicenseDir -or !(Test-Path $FfmpegLicenseDir)) {
        throw "Supply -FfmpegLicenseDir with the license and corresponding-source materials for the FFmpeg build you are distributing."
    }
}
cargo test --locked --lib --test editing
if ($LASTEXITCODE -ne 0) { throw "Rust editing tests failed." }
cargo build --locked --release
if ($LASTEXITCODE -ne 0) { throw "Native Windows build failed." }
New-Item -ItemType Directory -Path $packagePath | Out-Null
Copy-Item "target/release/frame.exe" (Join-Path $packagePath "Frame.exe")
Copy-Item "README.md" $packagePath
Copy-Item "THIRD-PARTY-LICENSES.txt" $packagePath
Copy-Item "assets/fonts/LICENSE-DejaVu.txt" $packagePath
if ($FfmpegDir) {
    foreach ($name in @("ffmpeg.exe", "ffprobe.exe", "ffplay.exe")) {
        $source = Join-Path $FfmpegDir $name
        if (Test-Path $source) { Copy-Item $source $packagePath }
    }
    Copy-Item $FfmpegLicenseDir (Join-Path $packagePath "FFmpeg-license-and-source") -Recurse
}
Get-ChildItem $packagePath -File -Recurse | Get-FileHash -Algorithm SHA256 | Select-Object Hash, Path | ConvertTo-Json | Set-Content (Join-Path $packagePath "SHA256-manifest.json")
$archive = "$packagePath.zip"
if (Test-Path $archive) { throw "Archive already exists; package folder was preserved, and the archive was not overwritten." }
Compress-Archive -Path "$packagePath/*" -DestinationPath $archive
Write-Host "Built $archive. Launch Frame.exe. No Node.js, npm, Electron, or Python is required."
if (!$FfmpegDir) { Write-Host "This package expects FFmpeg and FFprobe on PATH. FFplay is optional for audio preview." }
