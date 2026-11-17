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

Build 124 validation and packaging are in progress. The Store listing will change to Bediz after Microsoft certifies and publishes the update. The preceding SkyImage release remains available until then.
