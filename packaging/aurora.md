# RuStore / Aurora OS publishing — notes

Notes on how Filegram ships to the Aurora OS section of RuStore
(<https://console.rustore.ru/aurora-apps>). The operator's cheat sheet, like
`chocolatey.md`.

## What RuStore wants

Two RPMs per version, one per Aurora OS target:

- 32-bit: arch header `armv7hl`
- 64-bit: arch header `aarch64`

The upload validator checks the arch header strictly — `armv7` is rejected,
which is why the CI passes `--arch armv7hl` to cargo-generate-rpm.

## What CI does

`aurora.yml` (called from `release.yml`, or dispatched manually with a
version) builds both RPMs on the arm64 runner and attaches them to the GitHub
release as `filegram-aurora-aarch64.rpm` and `filegram-aurora-armv7hl.rpm`.

## What stays manual

RuStore has no public API for Aurora apps — the public API and the rustore
CLI cover Android (APK/AAB) only, and the dev console authenticates through
an httpOnly VK ID session. So publishing a new version is a console errand:

1. Download the two `filegram-aurora-*.rpm` assets from the release.
2. Console → Приложения → Аврора → Filegram → «Загрузить версию».
3. 32-бит поле takes the armv7hl file, 64-бит takes the aarch64 one.
   Both are required — the submit button stays disabled with only one.
4. Screenshots are portrait 9:16 (1080x1920 JPG recommended), minimum 3.
   Publication mode «Автоматически» publishes on moderator approval.

## Signing

RuStore warns that RPMs must be signed with an Aurora OS developer
certificate to pass publication. Unsigned packages upload and go to
moderation fine; the certificate request is filed through the link in that
warning banner. Once a certificate exists, the packages need signing before
upload — that step is not in CI yet.
