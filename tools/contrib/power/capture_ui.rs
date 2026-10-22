use std::ffi::{c_int, c_void};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Command;

unsafe extern "C" {
    fn mmap(
        addr: *mut c_void,
        len: usize,
        prot: c_int,
        flags: c_int,
        fd: c_int,
        offset: i64,
    ) -> *mut c_void;
    fn munmap(addr: *mut c_void, len: usize) -> c_int;
}

fn read_u32(memory: &mut File, address: u64) -> io::Result<u32> {
    let mut bytes = [0; 4];
    memory.seek(SeekFrom::Start(address))?;
    memory.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn require(value: u32, expected: u32, field: &str) -> io::Result<()> {
    if value != expected {
        return Err(io::Error::other(format!("unexpected {field}: {value:#x}")));
    }
    Ok(())
}

fn abs_pid() -> io::Result<u32> {
    let processes: Vec<_> = fs::read_dir("/proc")?
        .filter_map(Result::ok)
        .filter(|entry| {
            fs::read_to_string(entry.path().join("comm"))
                .ok()
                .as_deref()
                == Some("abs\n")
        })
        .collect();
    if processes.len() != 1 {
        return Err(io::Error::other("expected one abs process"));
    }
    let path = processes[0].path();
    let hash = Command::new("md5sum").arg(path.join("exe")).output()?;
    if !hash.status.success()
        || !String::from_utf8_lossy(&hash.stdout).starts_with("1a3fdfa8379d75a802190b48ea951e9a ")
    {
        return Err(io::Error::other("unexpected abs firmware"));
    }
    processes[0]
        .file_name()
        .to_string_lossy()
        .parse()
        .map_err(io::Error::other)
}

fn display_memory() -> io::Result<(u64, u64)> {
    let settings = fs::read_to_string("/tmp/directfbrc")?;
    let phys = settings
        .lines()
        .find_map(|line| line.strip_prefix("video-phys=0x"))
        .ok_or_else(|| io::Error::other("video-phys is missing"))?;
    let length = settings
        .lines()
        .find_map(|line| line.strip_prefix("video-length="))
        .ok_or_else(|| io::Error::other("video-length is missing"))?;
    let phys = u64::from_str_radix(phys, 16).map_err(io::Error::other)?;
    let length = length.parse().map_err(io::Error::other)?;
    if phys != 0x29000000 || length != 385875968 {
        return Err(io::Error::other("unexpected display memory range"));
    }
    Ok((phys, length))
}

fn run(output: &Path) -> io::Result<()> {
    let pid = abs_pid()?;
    let mut memory = File::open(format!("/proc/{pid}/mem"))?;
    let table = 0x13696d0;
    require(read_u32(&mut memory, table)?, 1, "surface validity")?;
    require(read_u32(&mut memory, table + 4)?, 1, "surface ID")?;
    require(read_u32(&mut memory, table + 8)?, 1, "window ID")?;
    require(
        read_u32(&mut memory, table + 16)?,
        0x4011040b,
        "pixel format",
    )?;
    require(read_u32(&mut memory, table + 20)?, 1280, "surface width")?;
    require(read_u32(&mut memory, table + 24)?, 720, "surface height")?;
    let surface = u64::from(read_u32(&mut memory, table + 12)?);
    require(
        read_u32(&mut memory, surface + 4)?,
        0x0b15171d,
        "interface magic",
    )?;
    let data = u64::from(read_u32(&mut memory, surface)?);
    let core = u64::from(read_u32(&mut memory, data + 0x68)?);
    require(
        read_u32(&mut memory, core + 0x10)?,
        0x1a0c1917,
        "core magic",
    )?;
    require(read_u32(&mut memory, core + 0xb4)?, 1280, "core width")?;
    require(read_u32(&mut memory, core + 0xb8)?, 720, "core height")?;
    require(
        read_u32(&mut memory, core + 0xbc)?,
        0x4011040b,
        "core pixel format",
    )?;
    require(read_u32(&mut memory, core + 0x13c)?, 1, "buffer count")?;
    let buffer = u64::from(read_u32(&mut memory, core + 0x124)?);
    require(read_u32(&mut memory, buffer)?, 0x13142617, "buffer magic")?;
    let allocation = u64::from(read_u32(&mut memory, buffer + 0x10)?);
    require(
        read_u32(&mut memory, allocation)?,
        0x0c123c1e,
        "allocation magic",
    )?;
    require(
        read_u32(&mut memory, allocation + 0x10)?,
        buffer as u32,
        "allocation buffer",
    )?;
    require(
        read_u32(&mut memory, allocation + 0x14)?,
        core as u32,
        "allocation surface",
    )?;
    let size = 1280 * 720;
    require(
        read_u32(&mut memory, allocation + 0x20)?,
        0xe4000,
        "allocation size",
    )?;
    let offset = u64::from(read_u32(&mut memory, allocation + 0x24)?);
    let palette = u64::from(read_u32(&mut memory, core + 0x15c)?);
    require(
        read_u32(&mut memory, palette + 0x90)?,
        0x04240603,
        "palette magic",
    )?;
    require(read_u32(&mut memory, palette + 0x94)?, 256, "palette size")?;
    let entries = u64::from(read_u32(&mut memory, palette + 0x98)?);
    let mut colors = [0; 1024];
    memory.seek(SeekFrom::Start(entries))?;
    memory.read_exact(&mut colors)?;

    let (phys, length) = display_memory()?;
    if offset
        .checked_add(size as u64)
        .is_none_or(|end| end > length)
    {
        return Err(io::Error::other("surface exceeds display memory"));
    }
    let maps = fs::read_to_string(format!("/proc/{pid}/maps"))?;
    let mapped = maps.lines().any(|line| {
        let mut fields = line.split_whitespace();
        let Some(range) = fields.next() else {
            return false;
        };
        let _permission = fields.next();
        let Some(start_offset) = fields.next() else {
            return false;
        };
        let Some((start, end)) = range.split_once('-') else {
            return false;
        };
        fields.last() == Some("/dev/mem")
            && u64::from_str_radix(start_offset, 16).ok() == Some(phys)
            && u64::from_str_radix(end, 16)
                .ok()
                .zip(u64::from_str_radix(start, 16).ok())
                .is_some_and(|(end, start)| end - start >= length)
    });
    if !mapped {
        return Err(io::Error::other("display memory is not mapped by abs"));
    }
    let device = File::open("/dev/mem")?;
    let address = phys
        .checked_add(offset)
        .ok_or_else(|| io::Error::other("address overflow"))?;
    let address = i64::try_from(address).map_err(io::Error::other)?;
    let mapping = unsafe {
        mmap(
            std::ptr::null_mut(),
            size,
            1,
            1,
            device.as_raw_fd(),
            address,
        )
    };
    if mapping as isize == -1 {
        return Err(io::Error::last_os_error());
    }
    let pixels = unsafe { std::slice::from_raw_parts(mapping.cast::<u8>(), size) }.to_vec();
    if unsafe { munmap(mapping, size) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut image = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(output)?;
    image.write_all(b"P6\n1280 720\n255\n")?;
    let mut rgb = vec![0; size * 3];
    for (index, pixel) in pixels.iter().enumerate() {
        let color = usize::from(*pixel) * 4;
        rgb[index * 3..index * 3 + 3].copy_from_slice(&colors[color + 1..color + 4]);
    }
    image.write_all(&rgb)?;
    image.flush()?;
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = if args.len() == 1 {
        run(Path::new(&args[0]))
    } else {
        Err(io::Error::other("usage: capture_ui <output.ppm>"))
    };
    if let Err(error) = result {
        eprintln!("display capture: {error}");
        std::process::exit(1);
    }
}
