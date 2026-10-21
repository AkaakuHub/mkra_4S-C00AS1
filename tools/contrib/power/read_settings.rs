use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};

#[cfg(not(all(target_arch = "arm", target_pointer_width = "32")))]
compile_error!("requires ARM32 Linux O_DIRECT flags");

#[repr(align(4096))]
struct Buffer([u8; 131072]);

fn run() -> io::Result<()> {
    if std::env::args().len() != 1 {
        return Err(io::Error::other("usage: read_settings"));
    }
    let sectors: u64 = fs::read_to_string("/sys/class/block/mmcblk0gp3/size")?
        .trim()
        .parse()
        .map_err(io::Error::other)?;
    let size = sectors
        .checked_mul(512)
        .ok_or_else(|| io::Error::other("invalid device size"))?;
    if size != 33554432 {
        return Err(io::Error::other("unexpected settings device size"));
    }
    let mut device = OpenOptions::new()
        .read(true)
        .custom_flags(0x10000 | 0x8000)
        .open("/dev/mmcblk0gp3")?;
    if !device.metadata()?.file_type().is_block_device() {
        return Err(io::Error::other("settings source is not a block device"));
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open("/tmp/mt/settings-readback.img")?;
    let mut buffer = Box::new(Buffer([0; 131072]));
    let mut copied = 0;
    while copied < size {
        device.read_exact(&mut buffer.0)?;
        output.write_all(&buffer.0)?;
        copied += buffer.0.len() as u64;
    }
    output.flush()?;
    println!("copied {copied} bytes using O_DIRECT");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("settings readback: {error}");
        std::process::exit(1);
    }
}
