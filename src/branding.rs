//! Visible branding only. Package, storage and project identities stay MyImage.
#[cfg(target_os = "windows")]
pub const NAME: &str = "SkyImage";
#[cfg(not(target_os = "windows"))]
pub const NAME: &str = "MyImage";
