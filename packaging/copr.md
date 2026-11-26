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

The version must be numeric and must identify a published tag. If `VERSION` is omitted, the Makefile reads it from an exact `vX.Y.Z` tag on `HEAD`. For the initial 0.2.12 package, run the **COPR package** workflow with `version=0.2.12` and upload the resulting SRPM directly with `copr-cli build filegram <package.src.rpm>`; subsequent packages can use the SCM source above.
