# Homebrew package

Filegram is distributed on macOS via [Homebrew](https://brew.sh) as a **cask**
from a dedicated tap repository:
**[filegram/homebrew-tap](https://github.com/filegram/homebrew-tap)**.

User-facing install:

```sh
brew install --cask filegram/tap/filegram
```

or tap first, then install:

```sh
brew tap filegram/tap
brew install --cask filegram
```

After install the GUI lives in `/Applications/Filegram.app` **and** a `filegram`
command is available in the terminal (see the `binary` stanza below).

## How the cask works

The cask lives at `Casks/filegram.rb` in the `homebrew-tap` repo (not in this
repo). It installs the universal (arm64 + x86_64) `Filegram.app` from the
release `.dmg`. Key points:

- It targets a single release asset, **`filegram-macos-universal.dmg`**, which
  contains `Filegram.app` (a lipo'd binary at
  `Filegram.app/Contents/MacOS/filegram`).
- `app "Filegram.app"` installs the bundle; `binary "…/MacOS/filegram"`
  symlinks the executable into `$(brew --prefix)/bin/filegram` (on `PATH`), so
  the app is launchable from a terminal as `filegram`. Existing installs pick
  the symlink up on the next `brew upgrade --cask filegram`.
- `depends_on macos: :big_sur` mirrors `LSMinimumSystemVersion` 11.0 from
  `assets/macos/Info.plist`.
- `zap` removes the app's leftovers, keyed by the bundle id
  `io.github.stan220.filegram`.
- `livecheck { strategy :github_latest }` lets `brew livecheck` detect new
  releases (it only reports; it does not edit the cask — see automation below).

Cask shape (abridged):

```ruby
cask "filegram" do
  version "0.2.5"
  sha256 "<sha256 of filegram-macos-universal.dmg>"

  url "https://github.com/filegram/filegram-desktop/releases/download/v#{version}/filegram-macos-universal.dmg",
      verified: "github.com/filegram/filegram-desktop/"
  name "Filegram"
  desc "Disk space analyzer with an interactive treemap"
  homepage "https://github.com/filegram/filegram-desktop"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: :big_sur

  app "Filegram.app"
  binary "#{appdir}/Filegram.app/Contents/MacOS/filegram"

  zap trash: [
    "~/Library/Preferences/io.github.stan220.filegram.plist",
    "~/Library/Saved Application State/io.github.stan220.filegram.savedState",
  ]
end
```

## Updating on a new release (automated)

The cask bump is automated by `.github/workflows/homebrew.yml`, mirroring the
Chocolatey flow. It pulls `filegram-macos-universal.dmg` from the release,
computes its SHA-256, rewrites `version` + `sha256` in `Casks/filegram.rb`, and
pushes to the tap. It runs automatically at the end of a release (via
`release.yml`) and can also be triggered manually
(Actions → Homebrew → Run workflow) to bump or backfill a specific version.

Requires a **`HOMEBREW_TAP_TOKEN`** repository secret: a PAT with
`contents: write` on `filegram/homebrew-tap`. The job's `GITHUB_TOKEN` only
reaches *this* repo, not the tap, so the cross-repo push needs its own token.
Without the secret the job logs a warning and exits cleanly, so a release is
never blocked on it.

## Updating on a new release (manual fallback)

In the `homebrew-tap` repo, bump `Casks/filegram.rb`:

1. Set `version` to the new release (without the `v` prefix).
2. Update `sha256` (SHA-256 of `filegram-macos-universal.dmg`).

Get the checksum from the release without downloading:

```sh
gh release view v<version> --repo filegram/filegram-desktop \
  --json assets --jq '.assets[] | select(.name=="filegram-macos-universal.dmg") | .digest'
```

(or `shasum -a 256 <file>` after downloading). The `digest` field is already
`sha256:...`; strip the prefix.

Validate before pushing — `brew audit [path]` is disabled, so use `brew style`
plus an install from a local tap:

```sh
brew style --fix Casks/filegram.rb   # fixes ">= :big_sur" → ":big_sur", zap ordering
# install from a local tap to verify the dmg mounts and `filegram` links:
cp -R . "$(brew --repository)/Library/Taps/filegram/homebrew-tap"
brew install --cask filegram/tap/filegram && which filegram
```

Pushing to the `homebrew-tap` repo follows the same rule as the rest of the
`filegram` org: push over **HTTPS via `gh`** (the SSH default resolves to the
wrong account). `gh repo create --source` leaves an SSH `origin` — switch it and
push with the gh credential helper:

```sh
git remote set-url origin https://github.com/filegram/homebrew-tap.git
git -c credential.helper='!gh auth git-credential' push
```

## Official homebrew-cask (deferred)

Submitting the cask to the official `Homebrew/homebrew-cask` would let users
`brew install --cask filegram` without tapping first. It's deferred until the
project clears Homebrew's notability bar — at which point: fork
`homebrew-cask`, drop an equivalent cask into `Casks/f/filegram.rb`, and open a
PR. Updates there are handled by Homebrew's `BrewTestBot` autobump.
