param([Parameter(Mandatory)][string]$PackageDir)
$ErrorActionPreference = "Stop"
$folder = (Resolve-Path $PackageDir).Path
$temp = Join-Path ([System.IO.Path]::GetTempPath()) ("frame-media-test-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $temp | Out-Null
try {
    $encoders = & (Join-Path $folder "ffmpeg.exe") -hide_banner -encoders 2>&1
    if ($LASTEXITCODE -ne 0 -or !(($encoders -join "`n") -match "h264_nvenc")) { throw "Bundled FFmpeg must include h264_nvenc; no NVIDIA hardware is required to list it." }
    $sample = Join-Path $temp "sample.mp4"
    & (Join-Path $folder "ffmpeg.exe") -hide_banner -nostdin -v error -f lavfi -i "testsrc2=size=320x180:rate=30:duration=1" -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=1" -c:v libx264 -preset ultrafast -pix_fmt yuv420p -c:a aac -shortest $sample
    if ($LASTEXITCODE -ne 0) { throw "Bundled FFmpeg H.264/AAC encoding failed." }
    $info = & (Join-Path $folder "ffprobe.exe") -v error -show_streams -show_format -of json $sample
    if ($LASTEXITCODE -ne 0) { throw "Bundled FFprobe failed." }
    $probe = ($info -join "`n") | ConvertFrom-Json
    if (!($probe.streams | Where-Object codec_name -EQ "h264") -or !($probe.streams | Where-Object codec_name -EQ "aac")) { throw "Missing expected video/audio streams." }
    & (Join-Path $folder "ffmpeg.exe") -hide_banner -nostdin -v error -i $sample -f null -
    if ($LASTEXITCODE -ne 0) { throw "Bundled media decoding failed." }
    & (Join-Path $folder "ffplay.exe") -version
    if ($LASTEXITCODE -ne 0) { throw "Bundled FFplay could not load." }
    # Frame uses the media tools beside its own executable, with no PATH setup.
    $check = Start-Process -FilePath (Join-Path $folder "Frame.exe") -ArgumentList '--check-tools' -Wait -PassThru
    if ($check.ExitCode -ne 0) { throw "Frame could not discover its bundled media tools." }
    Write-Host "Windows media encode/decode, FFplay loading and Frame tool discovery passed."
} finally {
    Remove-Item $temp -Recurse -Force
}
