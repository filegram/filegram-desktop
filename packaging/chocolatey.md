# Chocolatey publishing — notes

Notes on how Filegram ships to the Chocolatey community feed. The package
source lives in `packaging/chocolatey/`; this file is the operator's cheat
sheet (kept local, like `scoop.md`).

## What the package is

A **portable** package — it ships no binary. At install time
`tools/chocolateyinstall.ps1` downloads the official Windows executable from
the matching GitHub release and verifies it against a pinned SHA-256 sum:

- 64-bit: `filegram-windows-x86_64.exe`
- 32-bit: `filegram-windows-i686.exe`

The `.exe` is saved as `filegram.exe` in the package tools dir, so Chocolatey
generates a `filegram` shim. A `filegram.exe.gui` marker is written so the shim
is windowed (no console window, returns immediately) — Filegram is a GUI app.

Install once published: `choco install filegram`.

## Files (`packaging/chocolatey/`)

- `filegram.nuspec` — package metadata (id `filegram`, MIT, GUI/treemap tags).
- `tools/chocolateyinstall.ps1` — downloads + checksum-verifies the binary,
  supports both arches (`url`/`checksum` = i686, `url64bit`/`checksum64` = x86_64).
- `tools/VERIFICATION.txt` + `tools/LICENSE.txt` — required by community
  moderation for any package that fetches binaries from the internet.

**Placeholders** `__VERSION__`, `__CHECKSUM64__`, `__CHECKSUM32__` in the nuspec,
install script and VERIFICATION.txt are filled in by CI before packing. The
checked-in files are templates — never pack them as-is.

## Publishing (CI does it)

`.github/workflows/chocolatey.yml` (Windows runner) downloads the two release
`.exe`s, computes their SHA-256 sums, substitutes the placeholders, then runs
`choco pack` + `choco push`. Two triggers:

- **Automatic** — `release.yml` calls it (`uses: ./.github/workflows/chocolatey.yml`,
  `secrets: inherit`) right after a release is published. New releases publish
  to Chocolatey with no manual step.
- **Manual** — Actions → **Chocolatey** → Run workflow → `version` = e.g. `0.2.5`.
  Use this to backfill an already-released version. It checks out the **default
  branch** (for the packaging files) and pulls binaries from the release by
  version, so it works even if the tag predates the packaging files.

The job no-ops with a warning if `CHOCO_API_KEY` is unset — a release is never
blocked on Chocolatey.

## Secret

- `CHOCO_API_KEY` — repo secret, a chocolatey.org API key for an account that
  owns (or may submit) the `filegram` package id. Set via
  `gh secret set CHOCO_API_KEY --repo filegram/filegram-desktop`.
- **Rotate after exposure**: reset the key on chocolatey.org (Account → API Keys)
  and re-set the secret if it ever leaks (e.g. pasted in chat).

## Moderation

Every pushed version goes through Chocolatey moderation (automated validation +
sometimes human review) before it appears on the feed — can take days. Track
status at https://ch0.co/moderation; notification emails go to the account
owner (check spam). Docs:
https://docs.chocolatey.org/en-us/community-repository/moderation/

## On a new release — checklist

Nothing manual in the normal path: `release.yml` → `chocolatey.yml` runs on its
own. Just confirm afterwards that the Chocolatey job went green and the new
version shows up in the moderation queue. To (re)publish a specific version by
hand, use the manual trigger above.

## History

- 2026-06-15 — package + workflow added (PR #52); `filegram 0.2.5` pushed
  successfully and entered the moderation queue.
