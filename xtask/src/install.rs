//! Verified, reversible local installation. Never edits preferences, permissions,
//! projects or captures. Distribution update links live on the GitHub Releases page.
use crate::{StagingDirectory, TaskResult, checked, cleanup};
use std::{fs, path::Path, process::Command};

fn text(command: &mut Command) -> TaskResult<String> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}
fn value(app: &Path, key: &str) -> TaskResult<String> {
    Ok(text(
        Command::new("/usr/bin/plutil")
            .args(["-extract", key, "raw", "-o", "-"])
            .arg(app.join("Contents/Info.plist")),
    )?
    .trim()
    .into())
}
fn verify(app: &Path) -> TaskResult<()> {
    if fs::symlink_metadata(app)?.file_type().is_symlink() {
        return Err("Refusing a linked app bundle".into());
    }
    checked(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(app),
        "Verify app signature",
    )?;
    if value(app, "CFBundleIdentifier")? != "com.myimage.app" {
        return Err("This is not the MyImage app bundle".into());
    }
    Ok(())
}
fn verify_requirement(app: &Path, expression: &str) -> TaskResult<()> {
    // codesign treats -R values as filenames unless an inline expression starts with '='.
    checked(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "-R"])
            .arg(format!("={expression}"))
            .arg(app),
        "Verify required signer",
    )
}
fn requirement(app: &Path) -> TaskResult<String> {
    let out = text(
        Command::new("/usr/bin/codesign")
            .args(["-dr", "-"])
            .arg(app),
    )?;
    Ok(out
        .lines()
        .find(|s| s.starts_with("designated =>"))
        .ok_or("Missing signing requirement")?
        .into())
}
fn stopped() -> TaskResult<()> {
    let result = Command::new("/usr/bin/pgrep")
        .args(["-x", "MyImage"])
        .output()?;
    match result.status.code() {
        Some(1) => Ok(()),
        Some(0) => Err(
            "Save your edits and quit SkyImage or MyImage before installing. No files were replaced.".into(),
        ),
        _ => Err("Cannot establish whether MyImage is running. Installation stopped.".into()),
    }
}
fn same_tree(left: &Path, right: &Path) -> TaskResult<bool> {
    let mut left_names = fs::read_dir(left)?
        .map(|p| p.map(|p| p.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut right_names = fs::read_dir(right)?
        .map(|p| p.map(|p| p.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    left_names.sort();
    right_names.sort();
    if left_names != right_names {
        return Ok(false);
    }
    for name in left_names {
        let a = left.join(&name);
        let b = right.join(&name);
        let ma = fs::symlink_metadata(&a)?;
        let mb = fs::symlink_metadata(&b)?;
        if ma.is_dir() && mb.is_dir() {
            if !same_tree(&a, &b)? {
                return Ok(false);
            }
        } else if !ma.is_file() || !mb.is_file() || !cleanup::same_file_contents(&a, &b)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn check_upgrade(incoming: u64, installed: u64) -> TaskResult<()> {
    if incoming <= installed {
        return Err("The incoming build must be newer than the installed build. Rollback is a separate, deliberate operation.".into());
    }
    Ok(())
}
// A rename must never overwrite an unrelated second installation.
fn installed_path(applications: &Path) -> TaskResult<std::path::PathBuf> {
    let current = applications.join("SkyImage.app");
    let legacy = applications.join("MyImage.app");
    let has_current = current.symlink_metadata().is_ok();
    let has_legacy = legacy.symlink_metadata().is_ok();
    match (has_current, has_legacy) {
        (true, true) => Err("Both SkyImage.app and MyImage.app exist. Keep one verified installation before upgrading.".into()),
        (false, true) => Ok(legacy),
        _ => Ok(current),
    }
}

fn publish_install(
    stage: &mut StagingDirectory,
    staged: &Path,
    installed: &Path,
    destination: &Path,
    validate: impl FnOnce(&Path) -> TaskResult<()>,
) -> TaskResult<()> {
    if installed != destination && destination.symlink_metadata().is_ok() {
        return Err("The renamed destination already exists; no files were replaced.".into());
    }
    let previous = stage.path.join("previous.app");
    let had_previous = installed.symlink_metadata().is_ok();
    // Retain recovery material if any move or verification fails.
    stage.remove_on_drop = false;
    if had_previous {
        fs::rename(installed, &previous)?;
    }
    if let Err(error) = fs::rename(staged, destination) {
        if had_previous {
            fs::rename(&previous, installed)?;
        }
        return Err(error.into());
    }
    if let Err(error) = validate(destination) {
        fs::rename(destination, stage.path.join("failed.app"))?;
        if had_previous {
            fs::rename(&previous, installed)?;
        }
        return Err(format!("Update verification failed and was rolled back: {error}").into());
    }
    stage.remove_on_drop = true;
    Ok(())
}

pub fn run(workspace: &Path, migration: bool, unnotarized: bool) -> TaskResult<()> {
    let source = workspace.join("target/bundle/SkyImage.app");
    let destination = Path::new("/Applications/SkyImage.app");
    let current = installed_path(Path::new("/Applications"))?;
    let installed = current.as_path();
    stopped()?;
    verify(&source)?;
    // Updates must be signed by this app's distributor, not merely any Apple developer.
    verify_requirement(
        &source,
        "anchor apple generic and identifier \"com.myimage.app\" and certificate leaf[subject.OU] = \"HXDYTBC6ZT\" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists",
    )?;
    if !unnotarized {
        checked(
            Command::new("/usr/sbin/spctl")
                .args(["--assess", "--type", "execute", "--verbose=2"])
                .arg(&source),
            "Check notarized distribution",
        )?;
    }
    let build: u64 = value(&source, "CFBundleVersion")?.parse()?;
    if installed.exists() {
        verify(installed)?;
        check_upgrade(build, value(installed, "CFBundleVersion")?.parse()?)?;
        if requirement(&source)? != requirement(installed)? && !migration {
            return Err("Signing identity changed. Inspect the new signer before allowing a one-time --allow-signing-migration; macOS may require a fresh Screen Recording grant.".into());
        }
    }
    cleanup::check_workspace(workspace)?;
    let _lock = cleanup::release_lock(workspace)?;
    let mut stage = StagingDirectory::create(Path::new("/Applications"))?;
    let staged = stage.path.join("SkyImage.app");
    checked(
        Command::new("/usr/bin/ditto").arg(&source).arg(&staged),
        "Stage update",
    )?;
    verify(&staged)?;
    if !same_tree(&source, &staged)? {
        return Err("Staged app differs from the verified release".into());
    }
    if installed.exists() {
        let backups = workspace.join("backups");
        fs::create_dir_all(&backups)?;
        let mut backup = StagingDirectory::create(&backups)?;
        let zip = backup.path.join("rollback.zip");
        checked(
            Command::new("/usr/bin/ditto")
                .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
                .arg(installed)
                .arg(&zip),
            "Create installed-app rollback",
        )?;
        let unpacked = backup.path.join("verify");
        checked(
            Command::new("/usr/bin/ditto")
                .args(["-x", "-k"])
                .arg(&zip)
                .arg(&unpacked),
            "Verify rollback archive",
        )?;
        if !same_tree(
            installed,
            &unpacked.join(installed.file_name().ok_or("Invalid installed app path")?),
        )? {
            return Err("Rollback archive differs from installed app".into());
        }
        backup.publish(
            &zip,
            &backups.join(format!(
                "MyImage-rollback-macos-{}.zip",
                std::env::consts::ARCH
            )),
            "previous.zip",
        )?;
    }
    stopped()?;
    publish_install(&mut stage, &staged, installed, destination, verify)?;
    println!(
        "Installed SkyImage build {build}. Captures, projects, settings and permission records were not modified."
    );
    if migration {
        println!(
            "First Developer ID upgrade: macOS may request Screen Recording permission again."
        );
    }
    Ok(())
}
pub fn notarize(workspace: &Path, profile: &str) -> TaskResult<()> {
    let bundle = workspace.join("target/bundle");
    let app = bundle.join("SkyImage.app");
    let zip = bundle.join(format!("MyImage-macos-{}.zip", std::env::consts::ARCH));
    verify(&app)?;
    checked(
        Command::new("xcrun")
            .args(["notarytool", "submit"])
            .arg(&zip)
            .args(["--keychain-profile", profile, "--wait"]),
        "Submit to Apple notarization",
    )?;
    // Stapling also refuses a rejected or pending submission.
    checked(
        Command::new("xcrun").args(["stapler", "staple"]).arg(&app),
        "Staple notarization ticket",
    )?;
    checked(
        Command::new("xcrun")
            .args(["stapler", "validate"])
            .arg(&app),
        "Validate stapled ticket",
    )?;
    checked(
        Command::new("/usr/sbin/spctl")
            .args(["--assess", "--type", "execute", "--verbose=2"])
            .arg(&app),
        "Verify Gatekeeper acceptance",
    )?;
    let mut stage = StagingDirectory::create(&bundle)?;
    let archived = stage.path.join("notarized.zip");
    checked(
        Command::new("/usr/bin/ditto")
            .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
            .arg(&app)
            .arg(&archived),
        "Archive stapled release",
    )?;
    stage.publish(&archived, &zip, "previous.zip")?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installs_and_renames_restore_the_previous_bundle_on_failure() {
        for previous_name in ["MyImage.app", "SkyImage.app"] {
            for succeeds in [true, false] {
                let root = tempfile::tempdir().unwrap();
                let old = root.path().join(previous_name);
                let new = root.path().join("SkyImage.app");
                fs::create_dir(&old).unwrap();
                fs::write(old.join("binary"), b"old").unwrap();
                assert_eq!(installed_path(root.path()).unwrap(), old);
                let mut stage = StagingDirectory::create(root.path()).unwrap();
                let incoming = stage.path.join("incoming.app");
                fs::create_dir(&incoming).unwrap();
                fs::write(incoming.join("binary"), b"new").unwrap();
                let result = publish_install(&mut stage, &incoming, &old, &new, |_| {
                    if succeeds {
                        Ok(())
                    } else {
                        Err("invalid signature".into())
                    }
                });
                assert_eq!(result.is_ok(), succeeds);
                if succeeds {
                    assert_eq!(old.exists(), old == new);
                    assert_eq!(fs::read(new.join("binary")).unwrap(), b"new");
                    assert_eq!(installed_path(root.path()).unwrap(), new);
                } else {
                    assert_eq!(new.exists(), old == new);
                    assert_eq!(fs::read(old.join("binary")).unwrap(), b"old");
                }
            }
        }
    }

    #[test]
    fn renamed_install_rejects_duplicate_destination_and_restores_failed_move() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("MyImage.app");
        let new = root.path().join("SkyImage.app");
        fs::create_dir(&old).unwrap();
        fs::write(old.join("binary"), b"old").unwrap();
        fs::create_dir(&new).unwrap();
        assert!(installed_path(root.path()).is_err());
        let mut stage = StagingDirectory::create(root.path()).unwrap();
        let missing = stage.path.join("missing.app");
        assert!(publish_install(&mut stage, &missing, &old, &new, |_| Ok(())).is_err());
        fs::remove_dir(&new).unwrap();
        assert!(publish_install(&mut stage, &missing, &old, &new, |_| Ok(())).is_err());
        assert_eq!(fs::read(old.join("binary")).unwrap(), b"old");
        assert!(!new.exists());
    }

    #[test]
    fn inline_requirement_is_parsed_and_enforced_by_codesign() {
        let apple_binary = Path::new("/usr/bin/true");
        assert!(verify_requirement(apple_binary, "anchor apple").is_ok());
        assert!(
            verify_requirement(apple_binary, "identifier \"com.myimage.nonexistent\"").is_err()
        );
    }
    #[test]
    fn update_rejects_same_build_and_downgrades() {
        assert!(check_upgrade(40, 39).is_ok());
        assert!(check_upgrade(40, 40).is_err());
        assert!(check_upgrade(39, 40).is_err());
    }
    #[test]
    fn rollback_comparison_detects_missing_extra_and_changed_files() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a");
        let b = root.path().join("b");
        fs::create_dir_all(a.join("Contents")).unwrap();
        fs::create_dir_all(b.join("Contents")).unwrap();
        fs::write(a.join("Contents/app"), b"old").unwrap();
        fs::write(b.join("Contents/app"), b"old").unwrap();
        assert!(same_tree(&a, &b).unwrap());
        fs::write(b.join("Contents/app"), b"new").unwrap();
        assert!(!same_tree(&a, &b).unwrap());
        fs::write(b.join("Contents/app"), b"old").unwrap();
        fs::write(b.join("extra"), b"x").unwrap();
        assert!(!same_tree(&a, &b).unwrap());
    }
}
