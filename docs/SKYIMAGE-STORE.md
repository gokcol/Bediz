# SkyImage Windows Store update

SkyImage is the Windows display name from version 3.0.1 (build 109), Store package 3.0.1.0. The Store description and About identify MyImage as the previous name. macOS retains MyImage.

SkyImage was reserved for the existing Partner Center product on 2026-11-15. Keep the MyImage (OG) reservation. The current Store package 3.0.0.0 is approved; certification for this update is separate.

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
