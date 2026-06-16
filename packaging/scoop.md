# Scoop package

Filegram is distributed on Windows via [Scoop](https://scoop.sh) from a dedicated
bucket repository: **[filegram/scoop-bucket](https://github.com/filegram/scoop-bucket)**.

User-facing install:

```powershell
scoop bucket add filegram https://github.com/filegram/scoop-bucket
scoop install filegram
```

## How the manifest works

The manifest lives at `bucket/filegram.json` in the `scoop-bucket` repo (not in
this repo). Key points:

- It targets the **zip** release assets, not the bare `.exe`s:
  `filegram-windows-x86_64.zip` (`64bit`) and `filegram-windows-i686.zip`
  (`32bit`).
- Each zip contains a single executable at its root
  (`filegram-windows-<arch>.exe`), so the per-architecture `bin` aliases it to
  the `filegram` command and `shortcuts` create a "Filegram" Start-menu entry.
- `checkver: "github"` + `autoupdate` point at `.../download/v$version/...`.
  No hash URL is given, so on autoupdate Scoop downloads each zip and computes
  the SHA-256 itself.

Manifest shape (abridged):

```json
{
    "version": "0.2.5",
    "homepage": "https://github.com/filegram/filegram-desktop",
    "license": "MIT",
    "architecture": {
        "64bit": {
            "url": ".../v0.2.5/filegram-windows-x86_64.zip",
            "hash": "<sha256 of the x86_64 zip>",
            "bin": [["filegram-windows-x86_64.exe", "filegram"]],
            "shortcuts": [["filegram-windows-x86_64.exe", "Filegram"]]
        },
        "32bit": {
            "url": ".../v0.2.5/filegram-windows-i686.zip",
            "hash": "<sha256 of the i686 zip>",
            "bin": [["filegram-windows-i686.exe", "filegram"]],
            "shortcuts": [["filegram-windows-i686.exe", "Filegram"]]
        }
    },
    "checkver": "github",
    "autoupdate": { "architecture": {
        "64bit": { "url": ".../v$version/filegram-windows-x86_64.zip" },
        "32bit": { "url": ".../v$version/filegram-windows-i686.zip" }
    } }
}
```

## Updating on a new release

In the `scoop-bucket` repo, bump `bucket/filegram.json`:

1. Set `version` to the new release (without the `v` prefix).
2. Update **both** `hash` values (SHA-256 of the two zip assets).

Get the checksums from the release without downloading:

```sh
gh release view v<version> --repo filegram/filegram-desktop \
  --json assets --jq '.assets[] | select(.name|endswith(".zip")) | "\(.name) \(.digest)"'
```

(or `shasum -a 256 <file>` after downloading). The `digest` field is already
`sha256:...`; strip the prefix.

Pushing to the `scoop-bucket` repo follows the same rule as the rest of the
`filegram` org: push over **HTTPS via `gh`** (the SSH default resolves to the
wrong account). If a fresh clone's `origin` is SSH, switch it and push with the
gh credential helper:

```sh
git remote set-url origin https://github.com/filegram/scoop-bucket.git
git -c credential.helper='!gh auth git-credential' push
```

## Official Extras bucket (deferred)

Submitting the manifest to the official `ScoopInstaller/Extras` bucket would let
users install without `scoop bucket add`. It's deferred until the project gains
enough popularity to clear Extras' notability bar — at which point: fork Extras,
drop the same manifest into its `bucket/filegram.json`, and open a PR. Updates
there are handled by the Extras Excavator bot.
