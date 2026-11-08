#![allow(deprecated, unexpected_cfgs)]

use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::Path,
};

/// Keep the lock file open for the process lifetime. Never unlink it: another
/// process could already be waiting on the same inode.
pub struct InstanceLock {
    _file: File,
}

pub enum Instance {
    Primary(InstanceLock),
    Existing(i32),
}

impl InstanceLock {
    pub fn acquire() -> io::Result<Instance> {
        let uid = unsafe { libc::getuid() };
        // Terminal and launchd may have different TMPDIR environments.
        acquire_at(&Path::new("/tmp").join(format!("fts-{uid}.lock")))
    }
}

fn acquire_at(path: &Path) -> io::Result<Instance> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::WouldBlock {
            return Err(error);
        }
        let mut pid = String::new();
        file.read_to_string(&mut pid)?;
        return Ok(Instance::Existing(pid.trim().parse().unwrap_or(0)));
    }
    file.set_len(0)?;
    write!(file, "{}", std::process::id())?;
    Ok(Instance::Primary(InstanceLock { _file: file }))
}

pub fn activate_existing(pid: i32) {
    use cocoa::{appkit::NSRunningApplication, base::nil, foundation::NSAutoreleasePool};
    use objc::{class, msg_send, sel, sel_impl};
    if pid <= 0 {
        return;
    }
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(nil, pid);
        if app != nil {
            // LaunchServices sends a reopen event to the existing Dock app.
            let url: cocoa::base::id = msg_send![app, bundleURL];
            if url != nil {
                let workspace: cocoa::base::id = msg_send![class!(NSWorkspace), sharedWorkspace];
                let _: cocoa::base::BOOL = msg_send![workspace, openURL: url];
            }
        }
        pool.drain();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_is_blocked_and_stale_file_is_reusable() {
        let path =
            std::env::temp_dir().join(format!("fts-instance-test-{}.lock", std::process::id()));
        let Instance::Primary(lock) = acquire_at(&path).unwrap() else {
            panic!("first instance must own the lock");
        };
        assert!(
            matches!(acquire_at(&path).unwrap(), Instance::Existing(pid) if pid == std::process::id() as i32)
        );
        drop(lock);
        assert!(matches!(acquire_at(&path).unwrap(), Instance::Primary(_)));
        std::fs::remove_file(path).unwrap();
    }
}
