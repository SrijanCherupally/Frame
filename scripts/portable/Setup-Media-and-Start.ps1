param([switch]$SetupOnly)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-StrictMode -Version Latest

# Installs media tools only inside the extracted app folder. No admin or PATH changes.
$root = $PSScriptRoot
$application = Join-Path $root 'Frame.exe'
$toolDir = Join-Path $root 'MediaTools'
$downloadUrl = 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip'
$names = @('ffmpeg', 'ffprobe', 'ffplay')
$variables = @('FRAME_FFMPEG', 'FRAME_FFPROBE', 'FRAME_FFPLAY')
$stage = $null
$setupLock = $null

function Find-MediaTool([string]$name, [string]$variable) {
    $override = [Environment]::GetEnvironmentVariable($variable)
    if ($override -and (Test-Path -LiteralPath $override -PathType Leaf)) { return $override }
    foreach ($folder in @($root, $toolDir)) {
        $candidate = Join-Path $folder "$name.exe"
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
    }
    $command = Get-Command "$name.exe" -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($command) { return $command.Source }
    return $null
}

try {
    if (!(Test-Path -LiteralPath $application -PathType Leaf)) {
        throw 'Extract the entire ZIP first. Keep Start-Frame.cmd, Start-Frame.ps1 and Frame.exe together.'
    }
    $paths = @()
    $needsSetup = $false
    for ($i = 0; $i -lt $names.Count; $i++) {
        $path = Find-MediaTool $names[$i] $variables[$i]
        $paths += $path
        if (!$path) { $needsSetup = $true }
    }
    if ($needsSetup) {
        Write-Host 'First launch: downloading FFmpeg media tools. Future launches work offline.'
        Write-Host 'Download source: https://www.gyan.dev/ffmpeg/builds/'
        Write-Host 'Keep this window open until the download finishes. No administrator access is needed.'
        $setupLock = [IO.File]::Open((Join-Path $root '.frame-setup.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        $stage = Join-Path $root ('.frame-setup-' + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $stage | Out-Null
        $archive = Join-Path $stage 'ffmpeg.zip'
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -UseBasicParsing -Uri "$downloadUrl.sha256" -TimeoutSec 30 -OutFile (Join-Path $stage 'checksum.txt')
        $checksumText = Get-Content -LiteralPath (Join-Path $stage 'checksum.txt') -Raw
        $match = [regex]::Match($checksumText, '(?im)^\s*([a-f0-9]{64})(?:\s|$)')
        if (!$match.Success) { throw 'The publisher did not return a valid SHA-256 checksum. Nothing was installed.' }
        $expected = $match.Groups[1].Value
        Invoke-WebRequest -UseBasicParsing -Uri $downloadUrl -TimeoutSec 900 -OutFile $archive
        $actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
        if ($actual -ne $expected) { throw 'FFmpeg checksum mismatch. Nothing was installed; try again in case the publisher updated its release.' }
        $expanded = Join-Path $stage 'expanded'
        Expand-Archive -LiteralPath $archive -DestinationPath $expanded
        $binaries = @{}
        foreach ($name in $names) {
            $binary = @(Get-ChildItem -LiteralPath $expanded -Filter "$name.exe" -File -Recurse)
            if ($binary.Count -ne 1) { throw "The verified archive did not contain exactly one $name.exe." }
            $binaries[$name] = $binary[0].FullName
        }
        New-Item -ItemType Directory -Path $toolDir -Force | Out-Null
        foreach ($name in $names) { Copy-Item -LiteralPath $binaries[$name] -Destination (Join-Path $toolDir "$name.exe") -Force }
        # Retain the publisher's license, documentation and source information.
        $distribution = Split-Path (Split-Path $binaries['ffmpeg'] -Parent) -Parent
        foreach ($item in @(Get-ChildItem -LiteralPath $distribution | Where-Object { $_.Name -ne 'bin' })) {
            Copy-Item -LiteralPath $item.FullName -Destination $toolDir -Recurse -Force
        }
        @{source = $downloadUrl; sha256 = $actual} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $toolDir 'download.json') -Encoding UTF8
        $paths = @($names | ForEach-Object { Join-Path $toolDir "$_.exe" })
        Write-Host 'Media tools verified and installed.'
    }
    for ($i = 0; $i -lt $names.Count; $i++) { [Environment]::SetEnvironmentVariable($variables[$i], $paths[$i], 'Process') }
    if ($SetupOnly) { exit 0 }
    Write-Host 'Opening Frame...'
    Start-Process -FilePath $application -WorkingDirectory $root
} catch {
    Write-Host "Setup could not finish: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host 'Check your internet connection, close other setup windows, and extract the ZIP into a writable folder.'
    Write-Host 'You can also put ffmpeg.exe, ffprobe.exe and ffplay.exe beside Frame.exe and launch again.'
    exit 1
} finally {
    if ($stage -and (Test-Path -LiteralPath $stage)) { Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue }
    if ($setupLock) { $setupLock.Dispose() }
}
