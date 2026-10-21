use std::ffi::{CStr, c_char, c_int, c_ulong};
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::time::{Duration, Instant};

const TIOCCONS: c_ulong = 0x541d;
const O_NOCTTY: c_int = 0x100;
const O_NONBLOCK: c_int = 0x800;

unsafe extern "C" {
    fn posix_openpt(flags: c_int) -> c_int;
    fn grantpt(fd: c_int) -> c_int;
    fn unlockpt(fd: c_int) -> c_int;
    fn ptsname_r(fd: c_int, buffer: *mut c_char, length: usize) -> c_int;
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
}

pub fn restore() -> io::Result<()> {
    let console = console()?;
    reset(&console)
}

fn console() -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .custom_flags(O_NOCTTY | O_NONBLOCK)
        .open("/dev/console")
}

fn reset(console: &File) -> io::Result<()> {
    if unsafe { ioctl(console.as_raw_fd(), TIOCCONS, 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

struct Redirect {
    console: File,
    _slave: File,
    active: bool,
}

impl Drop for Redirect {
    fn drop(&mut self) {
        if self.active {
            let _ = reset(&self.console);
        }
    }
}

pub fn capture(send: impl FnOnce() -> io::Result<()>) -> io::Result<Vec<u8>> {
    let fd = unsafe { posix_openpt(2 | O_NOCTTY | O_NONBLOCK) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut master = unsafe { File::from_raw_fd(fd) };
    if unsafe { grantpt(fd) } != 0 || unsafe { unlockpt(fd) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut name = [0 as c_char; 128];
    let result = unsafe { ptsname_r(fd, name.as_mut_ptr(), name.len()) };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result));
    }
    let path = unsafe { CStr::from_ptr(name.as_ptr()) }
        .to_str()
        .map_err(io::Error::other)?;
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(O_NOCTTY | O_NONBLOCK)
        .open(path)?;
    let console = console()?;
    if unsafe { ioctl(slave.as_raw_fd(), TIOCCONS, 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut redirect = Redirect {
        console,
        _slave: slave,
        active: true,
    };
    send()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut captured = Vec::new();
    let mut buffer = [0; 4096];
    let mut overflow = false;
    while Instant::now() < deadline {
        match master.read(&mut buffer) {
            Ok(0) => break,
            Ok(length) => {
                if captured.len() + length <= 512 * 1024 {
                    captured.extend_from_slice(&buffer[..length]);
                } else {
                    overflow = true;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    reset(&redirect.console)?;
    redirect.active = false;
    if overflow {
        return Err(io::Error::other("console output exceeded capture limit"));
    }
    Ok(captured)
}
