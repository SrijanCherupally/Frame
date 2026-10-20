param(
    [string]$FfmpegDir,
    [string]$FfmpegLicenseDir,
    [string]$OutputDir = "windows-package"
)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)
if ($env:OS -ne "Windows_NT") { throw "Run this script on Windows with Rust and Visual Studio C++ Build Tools installed." }
$packagePath = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $OutputDir))
$archive = "$packagePath.zip"
if ((Test-Path $packagePath) -or (Test-Path $archive)) { throw "Output already exists. Choose a new -OutputDir to preserve previous packages." }
if ($FfmpegDir) {
    foreach ($name in @("ffmpeg.exe", "ffprobe.exe", "ffplay.exe")) {
        if (!(Test-Path (Join-Path $FfmpegDir $name))) { throw "Missing $name in -FfmpegDir." }
    }
    if (!$FfmpegLicenseDir -or !(Test-Path $FfmpegLicenseDir -PathType Container)) {
        throw "Supply -FfmpegLicenseDir with the license and complete corresponding-source materials for your FFmpeg build."
    }
}
cargo test --locked --lib --test editing --test documents --test animation
if ($LASTEXITCODE -ne 0) { throw "Rust editing tests failed." }
cargo build --locked --release
if ($LASTEXITCODE -ne 0) { throw "Native Windows build failed." }
$seed = Join-Path ([System.IO.Path]::GetTempPath()) ("frame-package-" + [guid]::NewGuid().ToString("N"))
try {
    New-Item -ItemType Directory -Path $packagePath | Out-Null
    if ($FfmpegDir) {
        foreach ($name in @("ffmpeg.exe", "ffprobe.exe", "ffplay.exe")) { Copy-Item (Join-Path $FfmpegDir $name) $packagePath }
        Copy-Item $FfmpegLicenseDir (Join-Path $packagePath "Media-Licenses-and-Source") -Recurse
    } else {
        Expand-Archive -LiteralPath "downloads/Frame-Windows-x64.zip" -DestinationPath $seed
        foreach ($name in @("ffmpeg.exe", "ffprobe.exe", "ffplay.exe", "Media-Licenses", "Sources")) {
            $source = Join-Path $seed $name
            if (!(Test-Path $source)) { throw "The bundled ZIP is incomplete: $name missing. Restore downloads/Frame-Windows-x64.zip or supply your own media distribution." }
            Copy-Item $source $packagePath -Recurse
        }
    }
    Copy-Item "target/release/frame.exe" (Join-Path $packagePath "Frame.exe")
    Copy-Item "README.md", "THIRD-PARTY-LICENSES.txt", "assets/fonts/LICENSE-DejaVu.txt" $packagePath
    Copy-Item "scripts/portable/*" $packagePath
    Copy-Item "examples/projects" (Join-Path $packagePath "Templates") -Recurse
    Copy-Item "examples/data" (Join-Path $packagePath "Templates/Data") -Recurse
    New-Item -ItemType Directory -Path (Join-Path $packagePath "docs") | Out-Null
    Copy-Item "docs/screenshots" (Join-Path $packagePath "docs/screenshots") -Recurse
    Copy-Item "docs/*.md", "docs/*.txt" (Join-Path $packagePath "docs")
    $manifest = Get-ChildItem $packagePath -File -Recurse | Sort-Object FullName | ForEach-Object {
        $relative = $_.FullName.Substring($packagePath.Length + 1).Replace('\', '/')
        "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $relative
    }
    $manifest | Set-Content (Join-Path $packagePath "SHA256.txt") -Encoding utf8
    Compress-Archive -Path "$packagePath/*" -DestinationPath $archive
    Write-Host "Built $archive. Extract ALL files, then launch Frame.exe. No separate media download is required."
} finally {
    if (Test-Path $seed) { Remove-Item $seed -Recurse -Force }
}
