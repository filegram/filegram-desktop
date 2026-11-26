[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [Parameter(Mandatory = $true)]
    [ValidateSet('x64', 'x86')]
    [string]$Arch,
    [Parameter(Mandatory = $true)]
    [string]$PackageName,
    [Parameter(Mandatory = $true)]
    [string]$Publisher,
    [Parameter(Mandatory = $true)]
    [string]$PublisherDisplayName,
    [Parameter(Mandatory = $true)]
    [string]$Executable,
    [Parameter(Mandatory = $true)]
    [string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw 'Version must be X.Y.Z'
}
$executablePath = (Resolve-Path $Executable).Path
$root = Split-Path -Parent $PSScriptRoot
$null = New-Item -ItemType Directory -Path $OutputDirectory -Force
$stage = Join-Path $OutputDirectory "stage-$Arch"
if (Test-Path $stage) {
    throw "Staging directory already exists: $stage"
}
$packagePath = Join-Path $OutputDirectory "filegram-windows-$Arch.msix"
if (Test-Path $packagePath) {
    throw "Package already exists: $packagePath"
}
$null = New-Item -ItemType Directory -Path $stage
Copy-Item $executablePath (Join-Path $stage 'filegram.exe')
Copy-Item -Recurse (Join-Path $root 'packaging/windows/store/Assets') (Join-Path $stage 'Assets')
[xml]$manifest = Get-Content -Raw (Join-Path $root 'packaging/windows/store/AppxManifest.xml')
$manifest.Package.Identity.Name = $PackageName
$manifest.Package.Identity.Publisher = $Publisher
$manifest.Package.Identity.Version = "$Version.0"
$manifest.Package.Identity.ProcessorArchitecture = $Arch
$manifest.Package.Properties.PublisherDisplayName = $PublisherDisplayName
$manifestPath = Join-Path $stage 'AppxManifest.xml'
[System.IO.File]::WriteAllText($manifestPath, $manifest.OuterXml, [System.Text.UTF8Encoding]::new($false))
$makeAppx = Get-Command MakeAppx.exe -ErrorAction SilentlyContinue
$makeAppxPath = if ($makeAppx) {
    $makeAppx.Source
} else {
    Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Directory |
        Sort-Object Name -Descending |
        ForEach-Object { Join-Path $_.FullName 'x64/MakeAppx.exe' } |
        Where-Object { Test-Path $_ } |
        Select-Object -First 1
}
if (-not $makeAppxPath) {
    throw 'MakeAppx.exe was not found'
}
& $makeAppxPath pack /d $stage /p $packagePath
if ($LASTEXITCODE -ne 0) {
    throw "MakeAppx failed with exit code $LASTEXITCODE"
}
Get-Item $packagePath
