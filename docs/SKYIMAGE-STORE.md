# SkyImage Windows Store update

SkyImage is the Windows display name from version 3.0.1 (build 109), Store package 3.0.1.0. The Store description and About identify MyImage as the previous name. macOS also uses SkyImage from build 110.

SkyImage was reserved for the existing Partner Center product on 2026-11-15. Keep the MyImage (OG) reservation. Package 3.0.1.0 is live under the SkyImage name. The 3.1.0.0 update is in certification as of 17 November 2026.

Compatibility identities intentionally remain unchanged:

- Product ID: `9PL3HWB23J38`; https://apps.microsoft.com/detail/9PL3HWB23J38
- Package: `OrhanGkl.MyImageOG`
- Publisher: `CN=A9F0208B-CC43-472D-AE82-9FE4556DC935`
- Application ID: `MyImage`; executable: `MyImage.exe`
- Startup task: `MyImageStartup`; existing registry registration remains stable.
- Settings, recovery, Pictures/MyImage, .myimage projects and clipboard formats retain their existing identifiers.
- eframe application ID remains MyImage; only its visible window title changes.

`src/branding.rs` controls visible platform names. GitHub repository variable `MSIX_DISPLAY_NAME` is SkyImage. Package generation applies it to Properties, VisualElements and StartupTask display names. Keep runFullTrust as the only restricted capability; do not restore unvirtualizedResources.

Windows still checks Microsoft Store; macOS still checks the existing GitHub repository. GitHub release v3.0.1-build109 also packages MyImage for macOS and SkyImage for Windows. The Windows installer keeps its upgrade AppId and installation folder, and replaces its old MyImage shortcuts with SkyImage shortcuts.

## Release verification — 15 November 2026

- Release tag `v3.0.1-build109`: `0791b060eb93793e8cc5d67efb3fdfc2473a2a25`.
- Store package source: `b47dbe1613b7a0d9aeab682dc6fc19558b347956`. App source, Cargo metadata, release notes and Mac bundle metadata are identical to the release tag; later commits update documentation and standalone Windows installer tooling only.
- Store MSIX SHA-256: `5528a1ddb489f46b38509cbeddb2483cdca3cd90c9338eb1d55885523e1bcaf2`.
- Native CI: [release checks](https://github.com/gokcol/MyImage/actions/runs/43745766556); 678 app tests and 15 packaging tests passed.
- [Store package validation](https://github.com/gokcol/MyImage/actions/runs/43745370477), [disposable MSIX install/removal](https://github.com/gokcol/MyImage/actions/runs/43745358564), and [Windows installer lifecycle](https://github.com/gokcol/MyImage/actions/runs/43745797010) passed.
- Apple app notarization `bd163679-9aba-462d-86d6-44e64228bd7a`: Accepted.
- Apple DMG notarization `e7329e5a-c272-4d5c-9132-5ebac244e90e`: Accepted. Tickets, Gatekeeper and mounted DMG contents verified.
- Installed and launched `/Applications/MyImage.app`, version 3.0.1 build 109; What's New confirms the installed build. Existing data and permission records retained; previous app rollback archived.
- Partner Center submission 3, ID `1152921505702410521`, is **in certification**, with automatic publishing after approval. Store listing uses SkyImage; description, short description, release notes and reviewer notes mention the rename. MyImage remains a reserved name and a listing keyword. Store approval is pending.
- Windows interactive capture/device validation remains pending.

## Mac rename — build 110

- Source/tag `ae0d4700ecd573e3dd0f46683f1e6a5ac93498b3`, `v3.0.1-build110`.
- Installed as `/Applications/SkyImage.app`, replacing verified MyImage.app; rollback archived. Signature requirement and bundle identifier match build 109. No data or permission records modified.
- Native top menu and About verified: SkyImage, Previously MyImage, version 3.0.1 build 110.
- [Native CI](https://github.com/gokcol/MyImage/actions/runs/43753097970): formatting, Clippy, 678 app tests and 17 packaging tests passed. Installation tests cover name migration and failure rollback.
- Apple app notarization `1ce7121e-def3-4237-81c4-ecc56dd48aeb`: Accepted. DMG notarization `7f8adcbb-4ca7-44cb-baf0-0456d02523bd`: Accepted. Stapling, Gatekeeper, DMG checksum and mounted SkyImage.app verified.
- [Mac release](https://github.com/gokcol/MyImage/releases/tag/v3.0.1-build110) published separately; Windows build 109 and its Store submission remain the Windows release.

## Version 3.1 submission — 17 November 2026

- Public Store listing verified as **SkyImage**; Partner Center’s internal product label remains MyImage (OG).
- Submission 4, `1152921505702422705`, is **in certification** with automatic publishing after approval.
- Package `MyImage-3.1.0.0-store-x64.msix`: app 3.1.0 build 120, x64, Windows 10.0.19041.0 minimum, display name SkyImage, `runFullTrust` only. Identity and publisher are unchanged.
- SHA-256: `bb566c199884b7009b53e55bdc3512c71c5813ffc74f18b16c39d20387cae004`. Exact source: `3e22ea98ead94976f9bf8215fe5b14d75339e368`. [Store packaging checks](https://github.com/gokcol/MyImage/actions/runs/43996375491) passed; Partner Center validated the upload and replaced the superseded 3.0.1.0 package in this submission.
- English description, What’s New and reviewer notes describe the compact capture toolbar, optional scrolling after Region/Window selection, 100/75/50/25% output, editable hotkey and Quick Start close button.
- Certification is pending; this record does not claim the new package is live.
