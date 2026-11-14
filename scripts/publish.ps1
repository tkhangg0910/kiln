# Builds the release zip of the Rust Kilno: one kilno.exe in a Kilno\ folder, the same layout,
# names and checksum file as the C# build's scripts/publish.ps1, so scripts/install.ps1 (and the
# Update button of an installed C# Clayo) installs it unchanged.
# Unzipped into %LOCALAPPDATA%\Programs it lands at %LOCALAPPDATA%\Programs\Kilno.
# From the repo root:  .\scripts\publish.ps1     (builds and zips; uploads nothing)
$ErrorActionPreference = 'Stop'

$rust = Split-Path $PSScriptRoot -Parent
$root = $rust
$manifest = Join-Path $rust 'Cargo.toml'

# [workspace.package] version = "x.y.z"
$version = $null
$section = ''
foreach ($line in Get-Content $manifest) {
    if ($line -match '^\s*\[(.+)\]\s*$') { $section = $Matches[1]; continue }
    if ($section -eq 'workspace.package' -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $version = $Matches[1]; break }
}
if (-not $version) { throw "No [workspace.package] version in $manifest" }

$out = Join-Path $root 'publish'
$folder = Join-Path $out 'Kilno'
$zip = Join-Path $out "kilno-$version-win-x64.zip"
if (Test-Path $folder) { Remove-Item $folder -Recurse -Force }
if (Test-Path $zip) { Remove-Item $zip -Force }
if (Test-Path "$zip.sha256") { Remove-Item "$zip.sha256" -Force }

Push-Location $rust
try {
    cargo build --release -p kilno-app
    if ($LASTEXITCODE -ne 0) { throw 'cargo build --release failed' }
}
finally { Pop-Location }

$exe = Join-Path $rust 'target\release\kilno.exe'
if (-not (Test-Path $exe)) { throw "Missing from the build: $exe" }
New-Item -ItemType Directory -Force $folder | Out-Null
Copy-Item $exe (Join-Path $folder 'kilno.exe')

# Entry by entry, with forward slashes: Windows PowerShell 5.1's Compress-Archive and ZipFile
# both write backslashes, which some unzip tools turn into files named "Kilno\kilno.exe".
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::Open($zip, [IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($file in Get-ChildItem $folder -Recurse -File) {
        $name = 'Kilno/' + $file.FullName.Substring($folder.Length + 1).Replace('\', '/')
        [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $file.FullName, $name, [IO.Compression.CompressionLevel]::Optimal)
    }
}
finally { $archive.Dispose() }

# Uploaded beside the zip: install.ps1 refuses a zip whose hash is not this one.
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content "$zip.sha256" "$hash  $(Split-Path $zip -Leaf)" -Encoding ascii -NoNewline
$exeMb = (Get-Item (Join-Path $folder 'kilno.exe')).Length / 1MB
$zipMb = (Get-Item $zip).Length / 1MB
'Kilno {0}: kilno.exe {1:N1} MB in {2}, zipped to {3} ({4:N1} MB), SHA-256 {5}' -f $version, $exeMb, $folder, $zip, $zipMb, $hash
