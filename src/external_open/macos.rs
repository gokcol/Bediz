use eframe::egui;
use objc2::{
    MainThreadMarker, class, ffi, msg_send,
    runtime::{AnyClass, AnyObject, Imp, Sel},
    sel,
};
use objc2_foundation::{NSArray, NSURL};
use std::{
    cell::RefCell,
    fs::File,
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::net::UnixStream,
    },
    path::{Path, PathBuf},
    thread::{self, JoinHandle},
};

#[derive(Default)]
struct FileOpen {
    pending: Option<PathBuf>,
    context: Option<egui::Context>,
}

thread_local! {
    static FILE_OPEN: RefCell<FileOpen> = RefCell::new(FileOpen::default());
}

pub(crate) fn install_file_open() -> Result<(), String> {
    let _main_thread =
        MainThreadMarker::new().ok_or("Finder file opening must be set up on the main thread.")?;
    // SAFETY: winit has created the shared application and owns its delegate.
    let application: *mut AnyObject =
        unsafe { msg_send![class!(NSApplication), sharedApplication] };
    let delegate: *mut AnyObject = unsafe { msg_send![application, delegate] };
    let delegate = unsafe { delegate.as_ref() }.ok_or("Missing native file-open delegate.")?;
    let delegate_class = delegate.class();
    if delegate_class.name() != c"WinitApplicationDelegate" {
        return Err("The native application delegate does not support Bediz file opening.".into());
    }
    add_file_open_hook(delegate_class)?;
    // SAFETY: Reassigning the same delegate refreshes AppKit's cached optional
    // methods without changing ownership or the existing winit callbacks.
    unsafe {
        let _: () = msg_send![application, setDelegate: delegate];
    }
    Ok(())
}

fn add_file_open_hook(delegate_class: &AnyClass) -> Result<(), String> {
    let selector = sel!(application:openURLs:);
    type OpenUrls = extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, &NSArray<NSURL>);
    // SAFETY: The callback takes the delegate, selector, app and URL array
    // arguments required by AppKit's void-returning optional method.
    let implementation = unsafe { std::mem::transmute::<OpenUrls, Imp>(open_urls) };
    if let Some(existing) = delegate_class.instance_method(selector) {
        return if std::ptr::fn_addr_eq(existing.implementation(), implementation) {
            Ok(())
        } else {
            Err("The application already has a different file-open handler.".into())
        };
    }
    // SAFETY: Register an absent method of the checked delegate class.
    let added = unsafe {
        ffi::class_addMethod(
            (delegate_class as *const AnyClass).cast_mut(),
            selector,
            implementation,
            c"v@:@@".as_ptr(),
        )
    };
    added
        .as_bool()
        .then_some(())
        .ok_or_else(|| "Could not install the Finder file-open handler.".into())
}

extern "C-unwind" fn open_urls(
    _delegate: *mut AnyObject,
    _selector: Sel,
    _application: *mut AnyObject,
    urls: &NSArray<NSURL>,
) {
    // Bediz keeps one open project, so keep the last file as in the
    // external launch queue. The normal open path handles errors and unsaved edits.
    let Some(path) = urls
        .iter()
        .filter(|url| url.isFileURL())
        .filter_map(|url| url.to_file_path())
        .last()
    else {
        return;
    };
    let ctx = FILE_OPEN.with(|state| {
        let mut state = state.borrow_mut();
        state.pending = Some(path);
        state.context.clone()
    });
    if let Some(ctx) = ctx {
        crate::runtime::wake_root(&ctx);
    }
}

pub(crate) fn bind_file_open(ctx: &egui::Context) {
    FILE_OPEN.with(|state| state.borrow_mut().context = Some(ctx.clone()));
}

pub(crate) fn take_file_open() -> Option<PathBuf> {
    FILE_OPEN.with(|state| state.borrow_mut().pending.take())
}

pub(super) struct QueueWatcher {
    stop: Option<UnixStream>,
    worker: Option<JoinHandle<()>>,
}

impl QueueWatcher {
    pub(super) fn new(directory: &Path, wake: impl Fn() + Send + 'static) -> io::Result<Self> {
        let directory = File::open(directory)?;
        let (stop, receive_stop) = UnixStream::pair()?;
        // SAFETY: kqueue takes no arguments and returns an owned descriptor.
        let fd = unsafe { libc::kqueue() };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: This is the sole owner of the valid descriptor returned above.
        let queue = unsafe { OwnedFd::from_raw_fd(fd) };
        if unsafe { libc::fcntl(queue.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let watch = |descriptor, filter, fflags| libc::kevent {
            ident: descriptor,
            filter,
            flags: libc::EV_ADD | libc::EV_CLEAR,
            fflags,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        let changes = [
            watch(
                directory.as_raw_fd() as _,
                libc::EVFILT_VNODE,
                libc::NOTE_WRITE,
            ),
            watch(receive_stop.as_raw_fd() as _, libc::EVFILT_READ, 0),
        ];
        // SAFETY: The two registrations reference open descriptors. No result
        // buffer or timeout is needed when only adding filters.
        let added = unsafe {
            libc::kevent(
                queue.as_raw_fd(),
                changes.as_ptr(),
                2,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if added < 0 {
            return Err(io::Error::last_os_error());
        }

        let worker = thread::Builder::new()
            .name("bediz-open-queue".into())
            .spawn(move || {
                let (_directory, _receive_stop) = (directory, receive_stop);
                loop {
                    let mut events = [std::mem::MaybeUninit::<libc::kevent>::uninit(); 2];
                    // SAFETY: The kernel writes up to two events to the live buffer.
                    // The watch descriptors stay open on this thread until it exits.
                    let count = unsafe {
                        libc::kevent(
                            queue.as_raw_fd(),
                            std::ptr::null(),
                            0,
                            events.as_mut_ptr().cast(),
                            2,
                            std::ptr::null(),
                        )
                    };
                    if count < 0 {
                        if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                            continue;
                        }
                        break;
                    }
                    for event in events.iter().take(count as usize) {
                        // SAFETY: This slot was initialized by kevent.
                        let event = unsafe { event.assume_init() };
                        if event.filter == libc::EVFILT_READ {
                            return;
                        }
                        wake();
                    }
                }
            })?;
        Ok(Self {
            stop: Some(stop),
            worker: Some(worker),
        })
    }
}

impl Drop for QueueWatcher {
    fn drop(&mut self) {
        // Closing the other end makes the stop stream readable and wakes kevent.
        self.stop.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finder_open_delivered_before_first_frame_keeps_the_project_path() {
        use objc2::{rc::Retained, runtime::ClassBuilder};

        let class = ClassBuilder::new(c"BedizTestFileOpenDelegate", class!(NSObject))
            .unwrap()
            .register();
        add_file_open_hook(class).unwrap();
        // SAFETY: The test class inherits NSObject's constructor and our
        // hook receives this test URL array with the registered signature.
        let delegate: Retained<AnyObject> = unsafe { msg_send![class, new] };
        let path = Path::new("/tmp/Bediz Açılış Testi.myimage");
        let urls = NSArray::from_retained_slice(&[NSURL::from_file_path(path).unwrap()]);
        unsafe {
            let _: () = msg_send![
                &delegate,
                application: std::ptr::null_mut::<AnyObject>(),
                openURLs: &*urls
            ];
        }

        bind_file_open(&egui::Context::default());
        assert_eq!(take_file_open().as_deref(), Some(path));
        assert!(take_file_open().is_none());
    }
}
