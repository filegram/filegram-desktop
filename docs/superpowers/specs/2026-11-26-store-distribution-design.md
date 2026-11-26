# Store distribution design

## Goal

Publish Filegram 0.2.12 through AppImageHub, Arch User Repository, Microsoft Store, and Fedora COPR, while leaving repeatable packaging inputs for later releases.

## Constraints

- Do not change application behavior or add application dependencies.
- Reuse the signed-off release assets from `v0.2.12` and existing icons.
- Never commit credentials, publisher identifiers obtained from private account pages, or signing material.
- AUR uses the `filegram-bin` package name because it repackages an upstream binary.
- Microsoft Store uses MSIX with `runFullTrust`; account-specific `Identity.Name`, `Identity.Publisher`, and `PublisherDisplayName` must come from Partner Center and must not be guessed.
- COPR builds from the source archive for an exact `vX.Y.Z` tag.
- External registration, identity verification, and acceptance of legal terms require an existing authorized account session; any such blocker is reported rather than bypassed.

## Channels

### AppImageHub

Add a single `data/filegram` input containing `https://github.com/filegram/filegram-desktop` through a pull request to `AppImage/appimage.github.io`. Verify the catalog check and resulting page.

### AUR

Publish `filegram-bin` with a `PKGBUILD` that downloads `filegram-linux-x86_64.zip` from the matching GitHub release, verifies SHA-256, installs the binary, desktop entry, license, and icons. Include `.SRCINFO` and `LICENSE` as required by AUR. Keep a template in `packaging/aur/` and document the release update procedure.

### Microsoft Store

Add a Windows-only MSIX packaging workflow and manifest template. The workflow produces x64 and x86 `.msix` packages from released executables, plus Store logo assets. Partner Center identity values are supplied at submission time, not hard-coded from another package manager. Submit only if an authorized Partner Center account is available; otherwise deliver validated packages and the exact remaining manual step.

### Fedora COPR

Add `packaging/copr/filegram.spec` and `.copr/Makefile`. The spec builds the tagged Rust source and installs the same desktop integration as AUR. Create an SCM package using `make srpm` and enable current stable Fedora x86_64 chroots. Trigger and verify the initial build if an authorized Fedora account is available.

## Verification

- Validate shell scripts with `bash -n` and XML with Python's XML parser.
- Recompute AUR archive checksums from the published v0.2.12 asset and run `makepkg --printsrcinfo` where available.
- Run MSIX packaging on a Windows runner and inspect both packages before submission.
- Build the COPR SRPM locally or in COPR and require successful binary builds.
- Run `cargo test` and `git diff --check` before integrating the packaging branch.
