# Flatpak / Flathub packaging

Filegram is distributed on [Flathub](https://flathub.org) as
`io.github.filegram.Filegram`, built from source on Flathub's infrastructure.

```
flatpak install flathub io.github.filegram.Filegram
flatpak run io.github.filegram.Filegram
```

## Files

| File | Purpose |
| --- | --- |
| `io.github.filegram.Filegram.yml` | Flatpak manifest (builds from a release tag) |
| `io.github.filegram.Filegram.metainfo.xml` | AppStream metadata (name, description, screenshots, releases) |
| `io.github.filegram.Filegram.desktop` | Desktop entry, named and icon-keyed by app id |
| `cargo-sources.json` | Vendored Cargo dependencies for the offline build |
| `screenshot.png` | Screenshot referenced by the metainfo |
| `generate-cargo-sources.sh` | Regenerates `cargo-sources.json` from `Cargo.lock` |

## Why these files live here, not only in the Flathub repo

Flathub builds run **offline**, so all crates are vendored into
`cargo-sources.json`. The metainfo and the app-id-named desktop file are
installed by the manifest from this directory rather than pulled out of the
tagged source tree — that keeps release tags free of any Flatpak-specific
changes. Keeping the manifest here makes it the source of truth; the Flathub
repository mirrors it.

## Initial Flathub submission

1. Fork [`flathub/flathub`](https://github.com/flathub/flathub) and create a
   branch named `io.github.filegram.Filegram` off the `new-pr` branch.
2. Copy the five build inputs into the branch root:
   `io.github.filegram.Filegram.yml`, `io.github.filegram.Filegram.metainfo.xml`,
   `io.github.filegram.Filegram.desktop`, `cargo-sources.json`, `screenshot.png`.
3. Open a PR against the `new-pr` branch. The Flathub buildbot builds it and a
   reviewer checks the manifest. Once merged, Flathub creates the dedicated
   `flathub/io.github.filegram.Filegram` repository.

Test locally first (on Linux):

```
flatpak install flathub org.flatpak.Builder
flatpak run org.flatpak.Builder --force-clean --install --user build-dir \
  packaging/flatpak/io.github.filegram.Filegram.yml
flatpak run io.github.filegram.Filegram
```

## Releasing a new version

In the `flathub/io.github.filegram.Filegram` repository (not this one):

1. Bump `tag:` and `commit:` in the manifest to the new release tag.
2. Regenerate sources if `Cargo.lock` changed: `./generate-cargo-sources.sh`,
   then copy `cargo-sources.json` over.
3. Add a `<release>` entry to the metainfo.
4. Open a PR; the buildbot builds and publishes it.
