use crate::{console_capture, native_transport};

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{FileExt, OpenOptionsExt};

fn configuration(pid: u32) -> io::Result<[u8; 20]> {
    let memory = File::open(format!("/proc/{pid}/mem"))?;
    let mut quick_start = [0; 20];
    memory.read_exact_at(&mut quick_start, 0x0190acfc)?;
    let mut auto_off = [0; 4];
    memory.read_exact_at(&mut auto_off, 0x0190ac24)?;
    if quick_start[..4] != [255, 255, 0, 0]
        || quick_start[16..18] != [0x34, 0x12]
        || auto_off != [0; 4]
    {
        return Err(io::Error::other("expected verified always-on settings"));
    }
    Ok(quick_start)
}

pub fn run() -> io::Result<()> {
    let receiver = native_transport::receiver()?;
    let before = configuration(receiver.pid)?;
    let captured = console_capture::capture(|| {
        native_transport::send(&receiver, native_transport::Request::SaveQuickStart)
    })?;
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open("/tmp/mt/settings-save-console.log")?
        .write_all(&captured)?;
    if configuration(receiver.pid)? != before {
        return Err(io::Error::other("settings changed during native save"));
    }
    println!("native settings save requested; verify storage readback");
    Ok(())
}
