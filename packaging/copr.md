# Fedora COPR

The COPR package builds Filegram from the source archive of an exact `vX.Y.Z` tag. `packaging/copr/filegram.spec` contains the RPM recipe; `.copr/Makefile` implements COPR's `make srpm` source method.

## Project configuration

Create a COPR project named `filegram` with current stable Fedora x86_64 chroots, then add an SCM package with:

- **Clone URL:** `https://github.com/filegram/filegram-desktop`
- **Committish:** the release tag, for example `v0.2.13`
- **Spec File:** `packaging/copr/filegram.spec`
- **SRPM build method:** `make srpm`

Enable package auto-rebuild and connect the COPR GitHub webhook for tag creation. Because release tags are named `vX.Y.Z` rather than `filegram-X.Y.Z`, append `/filegram/` to the webhook URL so COPR knows which package to rebuild.

Install with:

```sh
sudo dnf copr enable <owner>/filegram
sudo dnf install filegram
```

## Manual SRPM check on Fedora

```sh
make -f .copr/Makefile srpm \
  VERSION=0.2.12 \
  outdir="$PWD/dist" \
  spec="$PWD/packaging/copr/filegram.spec"
```

The version must be numeric and must identify a published tag. The initial 0.2.12 build uses the bootstrap default because that release tag predates the COPR files. Subsequent packages should set `VERSION` from the exact release tag.
