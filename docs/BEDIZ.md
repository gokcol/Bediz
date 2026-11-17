# Bediz — previously SkyImage and MyImage

Bediz 3.1.0 build 125 fixes fill-color changes affecting already completed fills. Its Microsoft Store package version is 3.1.3.0. Mac and Windows release packages are built and verified. [Build 125 is published on GitHub](https://github.com/gokcol/MyImage/releases/tag/v3.1.0-build125); the Store update is submitted and in certification. See [build 125 validation and release status](RELEASE-125.md).

The Bediz branding introduced in build 124 is retained on Mac and Windows, using the Bediz name reserved on the existing Store product.

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

## Build 125 release status

Completed fills keep their colors when choosing the next fill color, opacity or transparency. Select an existing fill to edit it deliberately; the inspector identifies it as **Selected fill**. The local Mac fix build passed its targeted checks, was notarized and installed. Public release artifacts were rebuilt and verified from tagged source `167b420e6cf24c73c81708344ba065c8a849dc32` with Store metadata 3.1.3.0. The Mac app and DMG are notarized, and Windows packaging and installation checks passed.

GitHub build 125 is published; all ten published assets match the verified local SHA-256 digests. Microsoft Store submission **7**, `1152921505702428856`, is **in certification / pre-processing** with validated package **3.1.3.0**, verified on 17 November 2026. It will publish automatically after Microsoft approval. The Bediz listing, Quick Fill release notes, reviewer testing instructions and capability justification were saved and verified after reloading. Build 124 remains published in the Store. Final artifact hashes and workflow results are recorded in the published `release-validation.json`.

## Build 124 release record

[Bediz v3.1 build 124](https://github.com/gokcol/MyImage/releases/tag/v3.1.0-build124) is published for Mac and Windows from source `8ce50a39118233b10b1ba356ae30f31cafe8300b`.

- [Native checks](https://github.com/gokcol/MyImage/actions/runs/44085724096): 717 app and 19 packaging tests passed, 27 ignored; formatting and Clippy passed.
- [Windows installer](https://github.com/gokcol/MyImage/actions/runs/44085970161), [Store package](https://github.com/gokcol/MyImage/actions/runs/44085974683) and [disposable MSIX install/removal](https://github.com/gokcol/MyImage/actions/runs/44085724274) passed. Package display names are Bediz; startup remains disabled and the only restricted capability is `runFullTrust`.
- Apple app notarization `4e9bf9bf-2cc2-4c2f-a8a5-3d643f72dcf1` and DMG notarization `676b867c-5a54-4a4b-b3ee-be0de20da898` were accepted. Signatures, stapled tickets, Gatekeeper, ZIP contents and read-only DMG contents were verified.
- Installed `/Applications/Bediz.app`, replacing verified SkyImage build 123 with a verified rollback archive. Native menu, window title, About and What's New show Bediz build 124. The existing saved project reopened; projects, preferences and permission records were not modified by installation.
- All ten published assets match their verified local SHA-256 digests. The release includes `release-validation.json`.

Microsoft Store submission **6**, `1152921505702429111`, was confirmed **published** in Partner Center on 17 November 2026. The English listing is saved as **Bediz**; description, short description, release notes, reviewer notes and capability justification were updated and verified after reloading. Package `MyImage-3.1.2.0-store-x64.msix` was validated and replaced the previous package in this submission. SHA-256: `7da5c57348684ce72cd022eaf9e26c502241c51a2dc50ce72af44ff1ab97d07f`. Partner Center confirms this submission is currently live in the Store. Interactive Windows capture/device validation remains pending.
