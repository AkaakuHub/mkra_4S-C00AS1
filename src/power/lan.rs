use std::ffi::c_void;
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
}

pub struct Lan {
    fd: OwnedFd,
}

impl Lan {
    pub fn open() -> io::Result<Self> {
        if fs::read_to_string("/proc/sys/kernel/osrelease")?.trim() != "3.10.24"
            || fs::read_to_string("/sys/class/net/eth0/address")?.trim() != "f0:9f:fc:60:76:4a"
        {
            return Err(io::Error::other("unexpected receiver identity"));
        }
        let digest = Command::new("/usr/bin/md5sum")
            .arg("/usr/bin/abs")
            .output()?;
        if !digest.status.success()
            || !String::from_utf8_lossy(&digest.stdout)
                .starts_with("1a3fdfa8379d75a802190b48ea951e9a ")
        {
            return Err(io::Error::other("unexpected firmware"));
        }
        let fd = unsafe { socket(2, 2, 0) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let lan = Self {
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
        };
        let mut driver = [0; 49];
        driver[0] = 3;
        lan.request(0x8946, &mut driver)?;
        if driver[1].to_le_bytes() != *b"XC3\0" {
            return Err(io::Error::other("unexpected Ethernet driver"));
        }
        Ok(lan)
    }

    fn request(&self, number: u32, data: &mut [u32]) -> io::Result<()> {
        let mut interface = InterfaceRequest {
            name: [0; 16],
            data: data.as_mut_ptr(),
            padding: [0; 12],
        };
        interface.name[..4].copy_from_slice(b"eth0");
        unsafe {
            crate::sys::ioctl_ptr(
                self.fd.as_raw_fd(),
                number as i32,
                (&mut interface as *mut InterfaceRequest).cast::<c_void>(),
            )?;
        }
        Ok(())
    }

    pub fn pmt(&self) -> io::Result<u32> {
        let mut registers = [0, 0x2c, 0];
        self.request(0x89f1, &mut registers)?;
        Ok(registers[2])
    }

    pub fn software_power_down(&self) -> io::Result<bool> {
        let mut state = [0; 218];
        self.request(0x89f3, &mut state)?;
        match state[0xb0 / 4] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(io::Error::other("unexpected MAC power state")),
        }
    }

    pub fn clients_connected(&self) -> io::Result<bool> {
        let sockets = fs::read_to_string("/proc/net/tcp")?;
        Ok(sockets
            .lines()
            .skip(1)
            .any(|line| line.split_whitespace().nth(3) == Some("01")))
    }

    pub fn standby(&self) -> io::Result<()> {
        if self.software_power_down()? || self.pmt()? & 1 != 0 {
            return Err(io::Error::other("MAC is already in standby"));
        }
        if let Err(error) = self.request(0x89f6, &mut [1]) {
            self.restore()?;
            return Err(error);
        }
        if !self.software_power_down()? || self.pmt()? & 3 != 3 {
            self.restore()?;
            return Err(io::Error::other("MAC did not enter magic-packet standby"));
        }
        Ok(())
    }

    pub fn restore(&self) -> io::Result<bool> {
        let software_down = self.software_power_down()?;
        let hardware = self.pmt()?;
        if !software_down && hardware & 1 == 0 {
            return Ok(false);
        }
        if hardware & 1 == 0 {
            self.request(0x89f6, &mut [1])?;
        }
        self.request(0x89f6, &mut [2])?;
        if self.software_power_down()? || self.pmt()? & 1 != 0 {
            return Err(io::Error::other("MAC restoration did not complete"));
        }
        Ok(true)
    }
}
