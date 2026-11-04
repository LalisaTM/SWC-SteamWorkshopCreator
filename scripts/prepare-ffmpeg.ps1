$ErrorActionPreference = 'Stop'
$archive = Join-Path $PSScriptRoot '../vendor/ffmpeg/ffmpeg-9.0.2-essentials_build.zip'
$expected = '60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba'
New-Item -ItemType Directory -Force (Split-Path $archive) | Out-Null
if (!(Test-Path -LiteralPath $archive)) {
    $temporary = "$archive.download"
    Invoke-WebRequest -Uri 'https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.0.2-essentials_build.zip' -OutFile $temporary
    if ((Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
        throw 'FFmpeg archive checksum mismatch'
    }
    Move-Item -LiteralPath $temporary -Destination $archive -Force
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
    throw "FFmpeg archive checksum mismatch: $archive"
}
