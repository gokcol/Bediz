//! Single-user document activation for desktop applications such as Seyir.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::QueueWatcher;
#[cfg(target_os = "macos")]
pub(crate) use macos::{bind_file_open, install_file_open, take_file_open};
#[cfg(target_os = "windows")]
use notify::RecommendedWatcher as QueueWatcher;

const REQUEST_LIMIT: u64 = 128 * 1024;
const QUEUE_LIMIT: usize = 32;
const REQUEST_AGE: Duration = Duration::from_secs(120);

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum Request {
    Show,
    Open(PathBuf),
}

pub enum Startup {
    Primary(Inbox),
    Forwarded,
}

pub struct Inbox {
    directory: PathBuf,
    _lock: File,
    watcher: Option<QueueWatcher>,
    dirty: Arc<AtomicBool>,
    initial: Option<Request>,
}

/// Test and demo instances stay independent of the installed application.
pub fn start(path: Option<PathBuf>, background: bool) -> Result<Startup, String> {
    let request = path
        .map(|path| {
            let resolved = path
                .canonicalize()
                .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
            if resolved.is_file() {
                Ok(Request::Open(resolved))
            } else {
                Err("Choose an image file to open in Bediz.".to_owned())
            }
        })
        .transpose()?;
    // MSIX can redirect writes to AppData into a package-private directory.
    // Keep only the activation queue and lock in the shared Windows profile
    // so Store, Setup and portable launches all find the same running copy.
    #[cfg(target_os = "windows")]
    let directory = directories::BaseDirs::new()
        .ok_or("Cannot locate Bediz's document-open directory.")?
        .home_dir()
        .join(".bediz")
        .join("external-open-v1");
    #[cfg(target_os = "macos")]
    let directory = directories::ProjectDirs::from("com", "MyImage", "MyImage")
        .ok_or("Cannot locate Bediz's document-open directory.")?
        .data_local_dir()
        .join("external-open-v1");
    start_at(directory, request, background).map_err(|error| error.to_string())
}

fn start_at(directory: PathBuf, initial: Option<Request>, background: bool) -> io::Result<Startup> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory)?;
    let directory = directory.canonicalize()?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("instance.lock"))?;
    if try_lock(&lock)? {
        return Ok(Startup::Primary(Inbox {
            directory,
            _lock: lock,
            watcher: None,
            dirty: Arc::new(AtomicBool::new(true)),
            initial,
        }));
    }
    if let Some(request) = initial.or((!background).then_some(Request::Show)) {
        enqueue(&directory, &request)?;
    }
    Ok(Startup::Forwarded)
}

fn request_files(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "request") && entry.file_type()?.is_file() {
            result.push(path);
            if result.len() > QUEUE_LIMIT {
                break;
            }
        }
    }
    Ok(result)
}

fn enqueue(directory: &Path, request: &Request) -> io::Result<()> {
    if request_files(directory)?.len() >= QUEUE_LIMIT {
        return Err(io::Error::other("Too many pending Bediz open requests."));
    }
    let bytes = serde_json::to_vec(request).map_err(io::Error::other)?;
    if bytes.len() as u64 > REQUEST_LIMIT {
        return Err(io::Error::other("The image path is too long."));
    }
    let mut temporary = tempfile::Builder::new()
        .prefix("open-")
        .tempfile_in(directory)?;
    temporary.write_all(&bytes)?;
    temporary.flush()?;
    let destination = temporary.path().with_extension("request");
    temporary.persist_noclobber(destination)?;
    Ok(())
}

impl Inbox {
    pub fn watch(&mut self, ctx: &eframe::egui::Context) -> Result<(), String> {
        let ctx = ctx.clone();
        let dirty = Arc::clone(&self.dirty);
        let wake = move || {
            dirty.store(true, Ordering::Release);
            crate::runtime::wake_root(&ctx);
        };
        #[cfg(target_os = "macos")]
        let watcher = QueueWatcher::new(&self.directory, wake);
        #[cfg(target_os = "windows")]
        let watcher = watch_windows(&self.directory, wake);
        self.watcher = Some(watcher.map_err(|error| error.to_string())?);
        Ok(())
    }

    pub fn take_latest(&mut self) -> Option<Request> {
        let mut latest = self.initial.take();
        if !self.dirty.swap(false, Ordering::AcqRel) {
            return latest;
        }
        let mut files: Vec<_> = request_files(&self.directory)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|path| {
                let modified = path.metadata().ok()?.modified().ok()?;
                Some((modified, path))
            })
            .collect();
        files.sort_unstable();
        for (modified, path) in files {
            if modified.elapsed().unwrap_or_default() <= REQUEST_AGE
                && let Ok(file) = File::open(&path)
            {
                let mut bytes = Vec::new();
                if file.take(REQUEST_LIMIT + 1).read_to_end(&mut bytes).is_ok()
                    && bytes.len() as u64 <= REQUEST_LIMIT
                    && let Ok(request) = serde_json::from_slice(&bytes)
                {
                    match request {
                        Request::Open(ref path) if !path.is_absolute() || !path.is_file() => {}
                        Request::Show if latest.is_some() => {}
                        request => latest = Some(request),
                    }
                }
            }
            let _ = fs::remove_file(path);
        }
        latest
    }
}

#[cfg(target_os = "windows")]
fn watch_windows(
    directory: &Path,
    wake: impl Fn() + Send + 'static,
) -> notify::Result<QueueWatcher> {
    use notify::{Event, EventKind, RecursiveMode, Watcher};

    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        if result.is_ok_and(|event| {
            event.need_rescan()
                || (matches!(
                    event.kind,
                    EventKind::Any | EventKind::Create(_) | EventKind::Modify(_)
                ) && event
                    .paths
                    .iter()
                    .any(|path| path.extension().is_some_and(|ext| ext == "request")))
        }) {
            wake();
        }
    })?;
    watcher.watch(directory, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

#[cfg(target_os = "macos")]
fn try_lock(file: &File) -> io::Result<bool> {
    use std::os::fd::AsRawFd;

    // SAFETY: The descriptor belongs to the file retained in the primary Inbox.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock {
        Ok(false)
    } else {
        Err(error)
    }
}

#[cfg(target_os = "windows")]
fn try_lock(file: &File) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{
        Foundation::{ERROR_LOCK_VIOLATION, HANDLE},
        Storage::FileSystem::{LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx},
        System::IO::OVERLAPPED,
    };

    // SAFETY: The handle remains valid throughout the synchronous, non-pending
    // lock call. Closing the file releases the one-byte lock.
    match unsafe {
        LockFileEx(
            HANDLE(file.as_raw_handle()),
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            None,
            1,
            0,
            &mut OVERLAPPED::default(),
        )
    } {
        Ok(()) => Ok(true),
        Err(error) if error.code() == ERROR_LOCK_VIOLATION.to_hresult() => Ok(false),
        Err(error) => Err(io::Error::other(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_instance_receives_a_path_without_starting_a_second_editor() {
        let folder = tempfile::tempdir().unwrap();
        let directory = folder.path().join("queue");
        let image = folder.path().join("görsel with spaces.png");
        fs::write(&image, b"image fixture").unwrap();
        let Startup::Primary(mut first) = start_at(directory.clone(), None, false).unwrap() else {
            panic!("new instance was not primary");
        };
        assert!(matches!(
            start_at(directory.clone(), Some(Request::Open(image.clone())), false).unwrap(),
            Startup::Forwarded
        ));
        assert!(matches!(first.take_latest(), Some(Request::Open(path)) if path == image));
        assert!(first.take_latest().is_none());
        drop(first);
        assert!(matches!(
            start_at(directory, None, false).unwrap(),
            Startup::Primary(_)
        ));
    }

    #[test]
    fn a_background_relaunch_does_not_open_another_editor_or_show_the_first() {
        let folder = tempfile::tempdir().unwrap();
        let directory = folder.path().join("queue");
        let Startup::Primary(mut first) = start_at(directory.clone(), None, true).unwrap() else {
            panic!("new instance was not primary");
        };
        assert!(matches!(
            start_at(directory.clone(), None, true).unwrap(),
            Startup::Forwarded
        ));
        assert!(request_files(&directory).unwrap().is_empty());

        assert!(matches!(
            start_at(directory, None, false).unwrap(),
            Startup::Forwarded
        ));
        assert!(matches!(first.take_latest(), Some(Request::Show)));
    }

    #[test]
    fn a_request_wakes_the_editor_after_its_inbox_is_idle() {
        let folder = tempfile::tempdir().unwrap();
        let directory = folder.path().to_path_buf();
        let image = directory.join("next.png");
        fs::write(&image, b"image fixture").unwrap();
        let Startup::Primary(mut first) = start_at(directory.clone(), None, false).unwrap() else {
            panic!("new instance was not primary");
        };
        let ctx = eframe::egui::Context::default();
        let woken = Arc::new(AtomicBool::new(false));
        crate::runtime::install_root_waker(&ctx, woken.clone(), |_| {});
        first.watch(&ctx).unwrap();
        assert!(first.take_latest().is_none());
        assert!(matches!(
            start_at(directory, Some(Request::Open(image.clone())), false).unwrap(),
            Startup::Forwarded
        ));
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !woken.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(woken.load(Ordering::Acquire));
        assert!(matches!(first.take_latest(), Some(Request::Open(path)) if path == image));
    }
}
