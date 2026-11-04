$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$name = 'SWC-SteamWorkshopCreator-windows-x64.exe'
Copy-Item -LiteralPath (Join-Path $root 'target/release/steam-workshop-creator.exe') -Destination (Join-Path $dist $name) -Force
$hash = (Get-FileHash -LiteralPath (Join-Path $dist $name) -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath (Join-Path $dist "$name.sha256") -Value "$hash  $name" -Encoding utf8NoBOM
Copy-Item -LiteralPath (Join-Path $root 'LICENSE'),(Join-Path $root 'THIRD_PARTY_NOTICES.md') -Destination $dist -Force
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead((Join-Path $root 'vendor/ffmpeg/ffmpeg-9.0.2-essentials_build.zip'))
try {
    foreach ($file in @(@('LICENSE','FFmpeg-LICENSE.txt'),@('README.txt','FFmpeg-README.txt'))) {
        $entry = $archive.GetEntry("ffmpeg-9.0.2-essentials_build/$($file[0])")
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry,(Join-Path $dist $file[1]),$true)
    }
} finally { $archive.Dispose() }
