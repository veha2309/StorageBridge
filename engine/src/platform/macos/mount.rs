//! Use libfuse3 for mount ownership; fuser's libfuse2 mount API is disabled in macFUSE 5.3+.
use anyhow::{Context, Result, ensure};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    os::fd::{FromRawFd, OwnedFd},
};
#[repr(C)]
struct Args {
    argc: c_int,
    argv: *mut *mut c_char,
    allocated: c_int,
}
type New = unsafe extern "C" fn(*mut Args, *const c_void, usize, *mut c_void) -> *mut c_void;
type MountAt = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;
type Fd = unsafe extern "C" fn(*mut c_void) -> c_int;
type End = unsafe extern "C" fn(*mut c_void);
type FreeArgs = unsafe extern "C" fn(*mut Args);
struct Library(*mut c_void);
impl Library {
    fn symbol(&self, name: &CStr) -> Result<*mut c_void> {
        let symbol = unsafe { libc::dlsym(self.0, name.as_ptr()) };
        ensure!(
            !symbol.is_null(),
            "macFUSE is missing {}; install macFUSE 5.4+",
            name.to_string_lossy()
        );
        Ok(symbol)
    }
}

pub struct Mount {
    session: *mut c_void,
    unmount: End,
    destroy: End,
    _library: Library,
}
impl Drop for Mount {
    fn drop(&mut self) {
        unsafe {
            (self.unmount)(self.session);
            (self.destroy)(self.session);
        }
    }
}
pub fn available() -> bool {
    std::path::Path::new("/usr/local/lib/libfuse3.dylib").exists()
}
pub fn open(path: &CStr) -> Result<(Mount, OwnedFd)> {
    // Resolve through this handle so libfuse2 symbols linked by fuser cannot collide.
    let library = Library(unsafe {
        libc::dlopen(
            c"/usr/local/lib/libfuse3.dylib".as_ptr(),
            libc::RTLD_NOW | libc::RTLD_LOCAL | libc::RTLD_NODELETE,
        )
    });
    ensure!(
        !library.0.is_null(),
        "Cannot load macFUSE libfuse3; install macFUSE 5.4+"
    );
    let new: New = unsafe { std::mem::transmute(library.symbol(c"fuse_session_new_30")?) };
    let mount_at: MountAt = unsafe { std::mem::transmute(library.symbol(c"fuse_session_mount")?) };
    let fd: Fd = unsafe { std::mem::transmute(library.symbol(c"fuse_session_fd")?) };
    let unmount: End = unsafe { std::mem::transmute(library.symbol(c"fuse_session_unmount")?) };
    let destroy: End = unsafe { std::mem::transmute(library.symbol(c"fuse_session_destroy")?) };
    let free_args: FreeArgs =
        unsafe { std::mem::transmute(library.symbol(c"fuse_opt_free_args")?) };
    let strings = [
        CString::new("StorageBridge")?,
        CString::new("-o")?,
        // Finder treats non-local macFUSE volumes as servers and can hide them.
        CString::new(
            "fsname=StorageBridge,volname=Storage Bridge,local,allow_root,default_permissions,nosuid,nodev",
        )?,
    ];
    let mut pointers: Vec<_> = strings.iter().map(|s| s.as_ptr().cast_mut()).collect();
    pointers.push(std::ptr::null_mut());
    let mut args = Args {
        argc: strings.len() as c_int,
        argv: pointers.as_mut_ptr(),
        allocated: 0,
    };
    // libfuse explicitly supports a no-op session for mounting with null operations.
    // The 3.0 compatibility entry point uses this stable four-argument signature.
    let session = unsafe { new(&mut args, std::ptr::null(), 0, std::ptr::null_mut()) };
    unsafe {
        free_args(&mut args);
    }
    ensure!(!session.is_null(), "Cannot create macFUSE session");
    let owner = Mount {
        session,
        unmount,
        destroy,
        _library: library,
    };
    ensure!(
        unsafe { mount_at(session, path.as_ptr()) } == 0,
        "macFUSE mount failed: {}",
        std::io::Error::last_os_error()
    );
    let borrowed = unsafe { fd(session) };
    ensure!(borrowed >= 0, "macFUSE did not provide a device descriptor");
    let duplicate = unsafe { libc::fcntl(borrowed, libc::F_DUPFD_CLOEXEC, 0) };
    ensure!(
        duplicate >= 0,
        "Cannot duplicate macFUSE descriptor: {}",
        std::io::Error::last_os_error()
    );
    let descriptor = unsafe { OwnedFd::from_raw_fd(duplicate) };
    Ok((owner, descriptor))
}
pub fn wait_ready(receiver: std::sync::mpsc::Receiver<()>) -> Result<()> {
    receiver
        .recv_timeout(std::time::Duration::from_secs(15))
        .context("macFUSE did not initialize; check its installation and extension approval")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readiness_requires_kernel_initialization() {
        let (sender, receiver) = std::sync::mpsc::channel();
        drop(sender);
        assert!(wait_ready(receiver).is_err());
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send(()).unwrap();
        assert!(wait_ready(receiver).is_ok());
    }
}
