# Arch User Repository

Filegram is packaged as [`filegram-bin`](https://aur.archlinux.org/packages/filegram-bin) because the package installs the prebuilt upstream Linux binary.

```sh
yay -S filegram-bin
```

The AUR submission consists of `PKGBUILD`, `.SRCINFO`, and the packaging `LICENSE` from `packaging/aur/`. The application's MIT license is downloaded separately from the release tag and installed into `/usr/share/licenses/filegram-bin/`.

## Updating

1. Set `pkgver` and `pkgrel=1` in `PKGBUILD`.
2. Download the release zip and the tagged desktop file, icon, and application license. Replace all four `sha256sums` values with `sha256sum` output.
3. Regenerate metadata and validate on Arch Linux:

   ```sh
   makepkg --printsrcinfo > .SRCINFO
   makepkg -fsi
   namcap PKGBUILD filegram-bin-*.pkg.tar.zst
   ```

4. Test `filegram` and the desktop launcher, then push `PKGBUILD`, `.SRCINFO`, and `LICENSE` to `ssh://aur@aur.archlinux.org/filegram-bin.git`.

AUR requires an account with an authorized SSH key. Do not commit private keys to this repository.
