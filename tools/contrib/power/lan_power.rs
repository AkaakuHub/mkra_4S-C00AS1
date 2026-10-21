#[cfg(not(all(target_arch = "arm", target_pointer_width = "32")))]
compile_error!("this tool requires the verified ARM32 receiver");

#[path = "../../../src/power/lan.rs"]
mod lan;
mod lan_trial;
use std::io;

mod sys {
    use std::ffi::c_void;
    use std::io;

    unsafe extern "C" {
        fn ioctl(fd: i32, request: i32, ...) -> i32;
    }

    pub unsafe fn ioctl_ptr(fd: i32, request: i32, data: *mut c_void) -> io::Result<i32> {
        let result = unsafe { ioctl(fd, request, data) };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(result)
    }
}

fn run() -> io::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args != ["--status"] && args != ["--test-mac-wake"] {
        return Err(io::Error::other(
            "usage: lan_power --status | --test-mac-wake",
        ));
    }
    let lan = lan::Lan::open()?;
    if args == ["--test-mac-wake"] {
        return lan_trial::run(&lan);
    }
    let pmt = lan.pmt()?;
    println!(
        "pmt={pmt:#010x} power_down={} magic_enabled={} wake_frame_enabled={} software_power_down={} clients_connected={}",
        pmt & 1 != 0,
        pmt & 2 != 0,
        pmt & 4 != 0,
        lan.software_power_down()?,
        lan.clients_connected()?,
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("native LAN power: {error}");
        std::process::exit(1);
    }
}
