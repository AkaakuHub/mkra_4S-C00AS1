#[cfg(not(all(target_arch = "arm", target_pointer_width = "32")))]
compile_error!("this tool requires the verified ARM32 receiver");

mod lan_trial;

use std::fs;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::Command;

#[repr(C)]
struct InterfaceRequest {
    name: [u8; 16],
    data: *mut u32,
    padding: [u8; 12],
}

unsafe extern "C" {
    fn socket(domain: i32, kind: i32, protocol: i32) -> i32;
    fn ioctl(fd: i32, request: u32, data: *mut InterfaceRequest) -> i32;
}

fn verified_socket() -> io::Result<OwnedFd> {
    if fs::read_to_string("/proc/sys/kernel/osrelease")?.trim() != "3.10.24"
        || fs::read_to_string("/sys/class/net/eth0/address")?.trim() != "f0:9f:fc:60:76:4a"
    {
        return Err(io::Error::other("unexpected receiver identity"));
    }
    let digest = Command::new("/usr/bin/md5sum")
        .arg("/usr/bin/abs")
        .output()?;
    if !digest.status.success()
        || !String::from_utf8_lossy(&digest.stdout).starts_with("1a3fdfa8379d75a802190b48ea951e9a ")
    {
        return Err(io::Error::other("unexpected firmware"));
    }
    let fd = unsafe { socket(2, 2, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn request(fd: &OwnedFd, number: u32, data: &mut [u32]) -> io::Result<()> {
    let mut interface = InterfaceRequest {
        name: [0; 16],
        data: data.as_mut_ptr(),
        padding: [0; 12],
    };
    interface.name[..4].copy_from_slice(b"eth0");
    if unsafe { ioctl(fd.as_raw_fd(), number, &mut interface) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn pmt(fd: &OwnedFd) -> io::Result<u32> {
    let mut registers = [0, 0x2c, 0];
    request(fd, 0x89f1, &mut registers)?;
    Ok(registers[2])
}

fn software_power_down(fd: &OwnedFd) -> io::Result<bool> {
    let mut state = [0; 218];
    request(fd, 0x89f3, &mut state)?;
    match state[0xb0 / 4] {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(io::Error::other("unexpected MAC power state")),
    }
}

fn run() -> io::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args != ["--status"] && args != ["--test-mac-wake"] {
        return Err(io::Error::other(
            "usage: lan_power --status | --test-mac-wake",
        ));
    }
    let fd = verified_socket()?;
    let mut driver = [0; 49];
    driver[0] = 3;
    request(&fd, 0x8946, &mut driver)?;
    if driver[1].to_le_bytes() != *b"XC3\0" {
        return Err(io::Error::other("unexpected Ethernet driver"));
    }
    if args == ["--test-mac-wake"] {
        return lan_trial::run(&fd);
    }
    let pmt = pmt(&fd)?;
    println!(
        "pmt={pmt:#010x} power_down={} magic_enabled={} wake_frame_enabled={} software_power_down={}",
        pmt & 1 != 0,
        pmt & 2 != 0,
        pmt & 4 != 0,
        software_power_down(&fd)?,
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("native LAN power: {error}");
        std::process::exit(1);
    }
}
