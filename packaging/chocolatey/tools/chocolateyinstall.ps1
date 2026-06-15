$ErrorActionPreference = 'Stop'

$toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

# __VERSION__ and the __CHECKSUM*__ placeholders are filled in by the release
# CI before packing. The binaries are the same official assets attached to the
# GitHub release; the pinned SHA-256 sums guarantee the download is unmodified.
$packageArgs = @{
  packageName    = $env:ChocolateyPackageName
  fileFullPath   = Join-Path $toolsDir 'filegram.exe'

  url            = 'https://github.com/filegram/filegram-desktop/releases/download/v__VERSION__/filegram-windows-i686.exe'
  checksum       = '__CHECKSUM32__'
  checksumType   = 'sha256'

  url64bit       = 'https://github.com/filegram/filegram-desktop/releases/download/v__VERSION__/filegram-windows-x86_64.exe'
  checksum64     = '__CHECKSUM64__'
  checksumType64 = 'sha256'
}

Get-ChocolateyWebFile @packageArgs

# Filegram is a GUI application. The .gui marker tells Chocolatey's shim
# generator to build a windowed shim that returns control immediately instead
# of spawning a console window and blocking on the process.
Set-Content -LiteralPath (Join-Path $toolsDir 'filegram.exe.gui') -Value '' -NoNewline
