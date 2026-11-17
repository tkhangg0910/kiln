# Installs Kilno, or updates it, into %LOCALAPPDATA%\Programs\Kilno, then starts it.
#   irm https://raw.githubusercontent.com/tkhangg0910/kilno/main/scripts/install.ps1 | iex
# Kilno's Update button runs this too, as it quits. Settings live in %LOCALAPPDATA%\Kilno and are kept.
$ErrorActionPreference = 'Stop'
# Windows PowerShell's progress bar slows a download to a crawl.
$ProgressPreference = 'SilentlyContinue'
# Out of the install folder first: a shell standing in it (Kilno's working folder when it was
# started from Explorer) keeps it busy, and the swap below fails with "in use".
Set-Location ([IO.Path]::GetTempPath())

# The Update button names the folder the running Kilno is in, which may be elsewhere.
$dir = if ($env:KILNO_DIR) { $env:KILNO_DIR.TrimEnd('\') } else { Join-Path $env:LOCALAPPDATA 'Programs\Kilno' }
$programs = Split-Path $dir -Parent
$exe = Join-Path $dir 'kilno.exe'
# Unpacked next to the install, so the swap is a rename on one drive.
$staging = Join-Path $programs ('Kilno-update-' + [guid]::NewGuid().ToString('N'))
$zip = "$staging.zip"
$old = "$staging-old"

function Get-Kilno { @(Get-Process kilno -ErrorAction SilentlyContinue | Where-Object Path -eq $exe) }

# Run by hand, a running Kilno is left alone: the script cannot tell whether a session in it
# is mid-task. The Update button waits for those, then quits Kilno and runs this.
if (-not $env:KILNO_DIR -and (Get-Kilno)) {
    Write-Host 'Kilno is running. Quit it from the gear menu, then run this again.' -ForegroundColor Yellow
    return
}

try {
    $release = Invoke-RestMethod 'https://api.github.com/repos/tkhangg0910/kilno/releases/latest'
    $asset = $release.assets | Where-Object name -like '*-win-x64.zip' | Select-Object -First 1
    if (-not $asset) { throw "Kilno $($release.tag_name) has no win-x64 zip." }

    Write-Host "Downloading Kilno $($release.tag_name)..."
    New-Item -ItemType Directory -Force $programs | Out-Null
    Invoke-WebRequest $asset.browser_download_url -OutFile $zip -UseBasicParsing

    # The release carries the zip's SHA-256 (scripts/publish.ps1). Nothing is unpacked unless it matches.
    $sum = $release.assets | Where-Object name -eq "$($asset.name).sha256" | Select-Object -First 1
    if (-not $sum) { throw "Kilno $($release.tag_name) has no checksum for $($asset.name)." }
    Invoke-WebRequest $sum.browser_download_url -OutFile "$zip.sha256" -UseBasicParsing
    $want = (Get-Content "$zip.sha256" -Raw).Trim().Split()[0]
    $got = (Get-FileHash $zip -Algorithm SHA256).Hash
    if ($got -ne $want) { throw "The download doesn't match its checksum, so it wasn't installed. Try again later." }
    Expand-Archive $zip $staging -Force

    # The Update button started this as Kilno quit; it holds its files open until it is gone.
    # Nothing in it was busy, so one that hangs on the way out is closed.
    $running = Get-Kilno
    if ($running) {
        Write-Host 'Waiting for Kilno to close...'
        $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
        $running | Where-Object { -not $_.HasExited } | Stop-Process -Force
        $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
    }

    # Renaming the old folder away fails as a whole if anything in it is still in use, so a
    # failed update leaves the old Kilno as it was. A swap rather than an unzip over the top,
    # so files a new version dropped do not linger.
    if (Test-Path $dir) { Rename-Item $dir (Split-Path $old -Leaf) }
    Move-Item (Join-Path $staging 'Kilno') $dir
    Remove-Item $old -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "Kilno $($release.tag_name) is installed in $dir."
}
catch {
    # Put the old Kilno back if it was moved away and the new one never landed.
    if ((Test-Path $old) -and -not (Test-Path $dir)) { Rename-Item $old (Split-Path $dir -Leaf) }
    throw
}
finally {
    Remove-Item $zip, "$zip.sha256", $staging -Recurse -Force -ErrorAction SilentlyContinue
    # The new Kilno, or the old one again if the update failed.
    if (Test-Path $exe) { Start-Process $exe }
}
