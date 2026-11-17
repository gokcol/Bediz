# Bediz — previously SkyImage and MyImage

Bediz 3.1.0 build 124 renames the existing Mac and Windows app. The Microsoft Store package version is 3.1.2.0, using the Bediz name reserved on the existing product.

## Update continuity

- Store product: [9PL3HWB23J38](https://apps.microsoft.com/detail/9PL3HWB23J38); package `OrhanGkl.MyImageOG` and publisher unchanged.
- Mac bundle: `Bediz.app`, bundle identifier `com.myimage.app`, executable `MyImage`, same Developer ID identity.
- Windows executable, installer AppId and installation folder remain unchanged. Start menu, desktop, Open with and uninstall labels use Bediz.
- `.myimage` projects, Pictures/MyImage, preferences, recovery and startup identifiers remain unchanged.
- Windows Store update checks and Mac GitHub update checks continue using their existing endpoints. The repository and downloadable filenames retain MyImage for older clients.
- About identifies both earlier names. MyImage (OG), SkyImage and Bediz remain reserved for the same Store product.

## Upgrade checks

The Mac installer accepts one existing MyImage.app, SkyImage.app or Bediz.app, verifies identity/version and stages a replacement before switching names. It rejects multiple installed copies and preserves a verified rollback bundle. Migration and failure tests cover all three names.

The Windows installer removes legacy MyImage and SkyImage shortcuts during upgrade and creates Bediz shortcuts. Desktop shortcuts remain opt-in. MSIX validation checks all three display-name fields and preserves `runFullTrust` as the only restricted capability; startup is disabled by default.

## Release status

[Bediz v3.1 build 124](https://github.com/gokcol/MyImage/releases/tag/v3.1.0-build124) is published for Mac and Windows from source `8ce50a39118233b10b1ba356ae30f31cafe8300b`.

- [Native checks](https://github.com/gokcol/MyImage/actions/runs/44085724096): 717 app and 19 packaging tests passed, 27 ignored; formatting and Clippy passed.
- [Windows installer](https://github.com/gokcol/MyImage/actions/runs/44085970161), [Store package](https://github.com/gokcol/MyImage/actions/runs/44085974683) and [disposable MSIX install/removal](https://github.com/gokcol/MyImage/actions/runs/44085724274) passed. Package display names are Bediz; startup remains disabled and the only restricted capability is `runFullTrust`.
- Apple app notarization `4e9bf9bf-2cc2-4c2f-a8a5-3d643f72dcf1` and DMG notarization `676b867c-5a54-4a4b-b3ee-be0de20da898` were accepted. Signatures, stapled tickets, Gatekeeper, ZIP contents and read-only DMG contents were verified.
- Installed `/Applications/Bediz.app`, replacing verified SkyImage build 123 with a verified rollback archive. Native menu, window title, About and What's New show Bediz build 124. The existing saved project reopened; projects, preferences and permission records were not modified by installation.
- All ten published assets match their verified local SHA-256 digests. The release includes `release-validation.json`.

Microsoft Store submission **6**, `1152921505702429111`, is **in certification / pre-processing** as of 17 November 2026, with automatic publishing after approval. The English listing is saved as **Bediz**; description, short description, release notes, reviewer notes and capability justification were updated and verified after reloading. Package `MyImage-3.1.2.0-store-x64.msix` was validated and replaced the previous package in this submission. SHA-256: `7da5c57348684ce72cd022eaf9e26c502241c51a2dc50ce72af44ff1ab97d07f`. The public Store listing changes to Bediz after certification and publication; SkyImage build 123 remains live meanwhile. Interactive Windows capture/device validation remains pending.
