# Store Distribution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish Filegram through AppImageHub, AUR, Microsoft Store, and Fedora COPR with repeatable packaging for future releases.

**Architecture:** Keep channel-specific metadata under `packaging/` and isolate COPR's required entry point in `.copr/`. Use existing release assets for AppImageHub, AUR, and MSIX; use tagged source archives for COPR. Account-specific identities and credentials stay outside version control.

**Tech Stack:** GitHub CLI/API, AUR `PKGBUILD`, Windows MSIX/`MakeAppx`, RPM spec/COPR SCM, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-11-26-store-distribution-design.md`

## Global Constraints

- Initial public version is `0.2.12`; do not rewrite its tag or release assets.
- No application behavior changes or new application dependencies.
- No credentials or guessed Partner Center identity values in the repository.
- Every external submission must use an existing authorized account; personal verification remains a documented blocker.

## Review Focus

- A changed release asset invalidates AUR hashes: pin the exact v0.2.12 URL and verify the downloaded digest.
- Wrong MSIX identity blocks Store upload: require Partner Center values and validate the four-part `X.Y.Z.0` version.
- AUR prebuilt package naming: use `filegram-bin`, with `provides` and `conflicts` for `filegram`.
- COPR must never infer an unpublished tag: require an exact reachable `vX.Y.Z` tag before SRPM generation.
- Packaging files must not leak account secrets: inspect the final diff and run repository secret-safe checks before integration.

---

### Task 1: AppImageHub

**Files:**
- External: `data/filegram` in a fork of `AppImage/appimage.github.io`.

**Interfaces:**
- Consumes: the existing `filegram-linux-x86_64.AppImage` asset in `v0.2.12`.
- Produces: an upstream AppImageHub pull request and, after acceptance, a public catalog entry.

- [ ] Fork/clone the catalog and create branch `add-filegram`.
- [ ] Verify no existing `data/filegram`; write exactly `https://github.com/filegram/filegram-desktop` plus one newline.
- [ ] Commit and push the branch, then open a PR titled `Add Filegram` with the release and AppImage links.
- [ ] Record the PR URL and inspect automated checks; fix metadata errors if the catalog reports them.

### Task 2: AUR `filegram-bin`

**Files:**
- Create: `packaging/aur/PKGBUILD`
- Create: `packaging/aur/.SRCINFO`
- Create: `packaging/aur/LICENSE`
- Create: `packaging/aur.md`
- External: the `filegram-bin` AUR repository.

**Interfaces:**
- Consumes: `https://github.com/filegram/filegram-desktop/releases/download/v0.2.12/filegram-linux-x86_64.zip` and its published SHA-256.
- Produces: `makepkg --printsrcinfo` output and an AUR package installable as `filegram-bin`.

- [ ] Download the pinned archive and independently compare its SHA-256 with the release digest.
- [ ] Write `PKGBUILD` with `pkgname=filegram-bin`, `pkgver=0.2.12`, `pkgrel=1`, `arch=(x86_64)`, `provides=(filegram)`, and `conflicts=(filegram)`; install binary, desktop file, license, and icons.
- [ ] Generate `.SRCINFO`; add the AUR submission license and maintainer instructions.
- [ ] Validate package contents in an Arch environment if available, then push to AUR using the authorized account.
- [ ] Verify the AUR RPC record and installation metadata.

### Task 3: Microsoft Store MSIX

**Files:**
- Create: `packaging/windows/store/AppxManifest.xml`
- Create: `packaging/windows/store/Assets/*`
- Create: `scripts/package-msix.ps1`
- Create: `.github/workflows/msix.yml`
- Create: `packaging/microsoft-store.md`

**Interfaces:**
- Consumes: released x64/x86 `filegram.exe` files and Partner Center `Name`, `Publisher`, `PublisherDisplayName` parameters.
- Produces: `package-msix.ps1 -Version <X.Y.Z> -Arch <x64|x86> ...` and matching `.msix` packages.

- [ ] Create Store logo assets from the existing icon and an XML manifest template with `runFullTrust`, `Windows.FullTrustApplication`, and version `X.Y.Z.0`.
- [ ] Implement the PowerShell packager with mandatory identity parameters, architecture validation, executable staging, and `MakeAppx pack` invocation.
- [ ] Validate XML and PowerShell syntax locally; run the workflow on Windows to produce and inspect x64/x86 packages.
- [ ] If Partner Center is accessible, create the app using its assigned identity and submit the packages. Otherwise stop at the exact identity/submission blocker without inventing values.

### Task 4: Fedora COPR

**Files:**
- Create: `packaging/copr/filegram.spec`
- Create: `.copr/Makefile`
- Create: `packaging/copr.md`

**Interfaces:**
- Consumes: an exact `vX.Y.Z` tag and `make -f .copr/Makefile srpm outdir=<dir> spec=<spec-path> VERSION=<X.Y.Z>`.
- Produces: a COPR SRPM and binary RPM for supported Fedora x86_64 chroots.

- [ ] Write the spec with `Name: filegram`, MIT license, tagged source URL, Rust/Linux build dependencies, desktop integration, and `%cargo_build`/`%cargo_install` where supported.
- [ ] Implement `.copr/Makefile` to require a nonempty numeric version, fetch sources, substitute `__VERSION__`, and run `rpmbuild -bs` into `outdir`.
- [ ] Validate SRPM generation and binary contents in a Fedora/RPM environment.
- [ ] Create the authorized COPR project and SCM package with `make srpm`; trigger the v0.2.12 build and inspect every enabled chroot.

### Task 5: Integration and reporting

**Files:**
- Modify: `README.md` with only verified public installation commands.
- Review all created files and external URLs.

**Interfaces:**
- Consumes: verified results from Tasks 1–4.
- Produces: a clean packaging branch and a report distinguishing published, pending-review, and blocked channels.

- [ ] Run XML/shell validation, `cargo test`, and `git diff --check`.
- [ ] Update README only for channels whose public records exist; do not document pending or blocked submissions as available.
- [ ] Commit logical packaging changes, run the full suite on the final diff, and integrate into `master` without rewriting public history.
- [ ] Report each channel URL/status and any remaining account-specific manual action.
