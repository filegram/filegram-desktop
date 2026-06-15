# Chocolatey package

Source for the [`filegram`](https://community.chocolatey.org/packages/filegram)
Chocolatey package. The package is a *portable* wrapper: it downloads the
official `filegram-windows-{x86_64,i686}.exe` from the matching GitHub release
and verifies it against a pinned SHA-256 checksum. Chocolatey generates a GUI
`filegram` shim from the downloaded executable.

## Layout

- `filegram.nuspec` — package metadata.
- `tools/chocolateyinstall.ps1` — downloads + checksum-verifies the binary.
- `tools/VERIFICATION.txt` / `tools/LICENSE.txt` — required by community
  moderation for packages that fetch binaries.

`__VERSION__`, `__CHECKSUM64__` and `__CHECKSUM32__` are placeholders filled in
by CI before packing — the files are **not** meant to be packed as-is.

## Publishing

Done by `.github/workflows/chocolatey.yml`, which pins the checksums, runs
`choco pack` and `choco push`. It runs automatically at the end of a release
(via `release.yml`) and can also be triggered manually
(Actions → Chocolatey → Run workflow) to publish or backfill a specific
version.

Requires a `CHOCO_API_KEY` repository secret — a [chocolatey.org](https://community.chocolatey.org/)
API key for an account that owns (or may submit) the `filegram` package id.
New package versions go through Chocolatey moderation before they appear.
