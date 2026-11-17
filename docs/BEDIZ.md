# Bediz — previously SkyImage and MyImage

Bediz 3.1.0 build 127 adds editable canvas text/image paste and keeps existing Recents previews visible after capture. [Build 127 is published on GitHub](https://github.com/gokcol/MyImage/releases/tag/v3.1.0-build127) for Mac and Windows; the notarized Mac build is installed. Its Microsoft Store update is **in certification** as package **3.1.4.0**, submission **8**, with automatic publication after Microsoft approval. Build 125 / package **3.1.3.0**, submission **7**, was confirmed published in Partner Center on 17 November 2026. See [build 127 validation and release status](RELEASE-127.md).

The Bediz branding introduced in build 124 is retained on Mac and Windows, using the Bediz name reserved on the existing Store product.

## Update continuity

- Store product: [9PL3HWB23J38](https://apps.microsoft.com/detail/9PL3HWB23J38); package `OrhanGkl.MyImageOG` and publisher unchanged.
- Mac bundle: `Bediz.app`, bundle identifier `com.myimage.app`, executable `MyImage`, same Developer ID identity.
- Windows executable, installer AppId and installation folder remain unchanged. Start menu, desktop, Open with and uninstall labels use Bediz.
- `.myimage` projects, Pictures/MyImage, preferences, recovery and startup identifiers remain unchanged.
- Windows Store update checks and Mac GitHub update checks continue using their existing endpoints. The repository and downloadable filenames retain MyImage for older clients.
- About identifies both earlier names. MyImage (OG), SkyImage and Bediz remain reserved for the same Store product.

Partner Center has a separate dashboard name from the public Store listing name. The English listing already used **Bediz**; on 17 November 2026, **Manage app names → Bediz → Set as dashboard name** changed the remaining MyImage (OG) dashboard title and heading to Bediz. This does not change the registered package identity.

## Upgrade checks

The Mac installer accepts one existing MyImage.app, SkyImage.app or Bediz.app, verifies identity/version and stages a replacement before switching names. It rejects multiple installed copies and preserves a verified rollback bundle. Migration and failure tests cover all three names.

The Windows installer removes legacy MyImage and SkyImage shortcuts during upgrade and creates Bediz shortcuts. Desktop shortcuts remain opt-in. MSIX validation checks all three display-name fields and preserves `runFullTrust` as the only restricted capability; startup is disabled by default.

## Build 127 Store submission — 17 November 2026

The published GitHub tag `v3.1.0-build127` remains at source `9ebd8da0f43edf81ad9e4be5231dccdd6d506fba`. The Store package uses metadata commit `49cd0739ecfb78fac3223d0ba892ac6c4b6078c5`, which advances the MSIX version and bundled Store version to **3.1.4.0** while retaining app **3.1.0 build 127**. Application logic is unchanged from the published tag; existing GitHub binaries and the installed Mac app are retained.

Store submission **8**, `1152921505702431515`, is **in certification / pre-processing**, verified on 17 November 2026. The uploaded package passed Partner Center validation, and the Store packaging, disposable installation/removal and native checks passed. It will publish automatically after Microsoft approval. The English product name and Partner Center dashboard name are both Bediz.

## Build 125 release record

Completed fills keep their colors when choosing the next fill color, opacity or transparency. Select an existing fill to edit it deliberately; the inspector identifies it as **Selected fill**. The local Mac fix build passed its targeted checks, was notarized and installed. Public release artifacts were rebuilt and verified from tagged source `167b420e6cf24c73c81708344ba065c8a849dc32` with Store metadata 3.1.3.0. The Mac app and DMG are notarized, and Windows packaging and installation checks passed.

GitHub build 125 is published; all ten published assets match the verified local SHA-256 digests. Microsoft Store submission **7**, `1152921505702428856`, with package **3.1.3.0**, was confirmed **published** in Partner Center on 17 November 2026, replacing build 124 as the live Store version. The Bediz listing, Quick Fill release notes, reviewer testing instructions and capability justification were saved and verified after reloading. Final artifact hashes and workflow results are recorded in the published `release-validation.json`.

## Build 124 release record

[Bediz v3.1 build 124](https://github.com/gokcol/MyImage/releases/tag/v3.1.0-build124) is published for Mac and Windows from source `8ce50a39118233b10b1ba356ae30f31cafe8300b`.

- [Native checks](https://github.com/gokcol/MyImage/actions/runs/44085724096): 717 app and 19 packaging tests passed, 27 ignored; formatting and Clippy passed.
- [Windows installer](https://github.com/gokcol/MyImage/actions/runs/44085970161), [Store package](https://github.com/gokcol/MyImage/actions/runs/44085974683) and [disposable MSIX install/removal](https://github.com/gokcol/MyImage/actions/runs/44085724274) passed. Package display names are Bediz; startup remains disabled and the only restricted capability is `runFullTrust`.
- Apple app notarization `4e9bf9bf-2cc2-4c2f-a8a5-3d643f72dcf1` and DMG notarization `676b867c-5a54-4a4b-b3ee-be0de20da898` were accepted. Signatures, stapled tickets, Gatekeeper, ZIP contents and read-only DMG contents were verified.
- Installed `/Applications/Bediz.app`, replacing verified SkyImage build 123 with a verified rollback archive. Native menu, window title, About and What's New show Bediz build 124. The existing saved project reopened; projects, preferences and permission records were not modified by installation.
- All ten published assets match their verified local SHA-256 digests. The release includes `release-validation.json`.

Microsoft Store submission **6**, `1152921505702429111`, was confirmed **published** in Partner Center on 17 November 2026. The English listing was saved as **Bediz**; description, short description, release notes, reviewer notes and capability justification were updated and verified after reloading. Package `MyImage-3.1.2.0-store-x64.msix` was validated and replaced the previous package in this submission. SHA-256: `7da5c57348684ce72cd022eaf9e26c502241c51a2dc50ce72af44ff1ab97d07f`. Submission 7 subsequently replaced this Store release. Interactive Windows capture/device validation remains pending.
