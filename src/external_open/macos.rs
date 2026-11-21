use std::{
    fs::File,
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::net::UnixStream,
    },
    path::Path,
    thread::{self, JoinHandle},
};

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
