# Microsoft Store (MSIX)

`scripts/package-msix.ps1` builds an unsigned x64 or x86 MSIX package from a released Filegram executable. The Microsoft Store re-signs packages after certification, so a CA certificate is not required for Store submission.

## Prerequisites

Create or open the application in Partner Center and copy these exact values from **Product management → View app identity details**:

- `Identity.Name`
- `Identity.Publisher`
- `PublisherDisplayName`

Do not reuse the winget identifier or invent a publisher DN. These values are account-specific.

## Build both packages

Run **Actions → MSIX → Run workflow** with the released version and the three identity values. The workflow downloads `filegram-windows-x86_64.exe` and `filegram-windows-i686.exe` from the matching GitHub release and creates `filegram-msix-x64` and `filegram-msix-x86` artifacts.

For a local Windows build:

```powershell
./scripts/package-msix.ps1 `
  -Version 0.2.12 `
  -Arch x64 `
  -PackageName '<Identity.Name>' `
  -Publisher '<Identity.Publisher>' `
  -PublisherDisplayName '<PublisherDisplayName>' `
  -Executable './filegram-windows-x86_64.exe' `
  -OutputDirectory './dist'
```

Run Windows App Certification Kit against both packages before submission. Upload the `.msix` files to the application's Partner Center submission and complete the Store listing, screenshots, age rating, privacy policy, and certification steps.

The workflow intentionally does not publish automatically: Store submission requires the application identity and listing created in Partner Center.
