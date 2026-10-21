use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;

const DEVICE: &str = "/dev/spidev0.0";
const OUTPUT: &str = "/tmp/mt/as1-spi-rom.bin";
const SIZE: usize = 2 * 1024 * 1024;
const BLOCK: usize = 1024;
const SPEED_HZ: u32 = 1_000_000;
const SPI_IOC_MESSAGE_2: u32 = 0x4040_6b00;
const EXPECTED_ID: [u8; 3] = [0xef, 0x70, 0x15];

#[repr(C)]
struct SpiIocTransfer {
    tx_buf: u64,
    rx_buf: u64,
    len: u32,
    speed_hz: u32,
    delay_usecs: u16,
    bits_per_word: u8,
    cs_change: u8,
    pad: u32,
}

unsafe extern "C" {
    fn ioctl(fd: i32, request: u32, ...) -> i32;
}

fn transfer(file: &File, command: &[u8], response: &mut [u8]) -> io::Result<()> {
    let mut transfers = [
        SpiIocTransfer {
            tx_buf: command.as_ptr() as usize as u64,
            rx_buf: 0,
            len: command.len() as u32,
            speed_hz: SPEED_HZ,
            delay_usecs: 0,
            bits_per_word: 8,
            cs_change: 0,
            pad: 0,
        },
        SpiIocTransfer {
            tx_buf: 0,
            rx_buf: response.as_mut_ptr() as usize as u64,
            len: response.len() as u32,
            speed_hz: SPEED_HZ,
            delay_usecs: 0,
            bits_per_word: 8,
            cs_change: 0,
            pad: 0,
        },
    ];
    let result = unsafe { ioctl(file.as_raw_fd(), SPI_IOC_MESSAGE_2, transfers.as_mut_ptr()) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    if result as usize != command.len() + response.len() {
        return Err(io::Error::other(format!("short SPI transfer: {result}")));
    }
    Ok(())
}

fn read_block(file: &File, address: usize, data: &mut [u8]) -> io::Result<()> {
    if address + data.len() > SIZE || data.len() > BLOCK {
        return Err(io::Error::other("read outside ROM"));
    }
    let command = [
        0x03,
        (address >> 16) as u8,
        (address >> 8) as u8,
        address as u8,
    ];
    transfer(file, &command, data)
}

fn read_range(file: &File, address: usize, data: &mut [u8]) -> io::Result<()> {
    for (offset, block) in data.chunks_mut(BLOCK).enumerate() {
        read_block(file, address + offset * BLOCK, block)?;
    }
    Ok(())
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn run(probe_only: bool) -> io::Result<()> {
    let file = OpenOptions::new().read(true).open(DEVICE)?;
    for attempt in 1..=5 {
        let mut id = [0u8; 5];
        transfer(&file, &[0x9f], &mut id)?;
        if id[..3] != EXPECTED_ID {
            return Err(io::Error::other(format!("unexpected JEDEC ID: {id:02x?}")));
        }
        println!("JEDEC {attempt}/5: {id:02x?}");
    }
    for address in [0, 0x10000, 0x1ff000] {
        let mut first = [0u8; 256];
        let mut second = [0u8; 256];
        read_block(&file, address, &mut first)?;
        read_block(&file, address, &mut second)?;
        if first != second {
            return Err(io::Error::other(format!("unstable SPI data at {address:#x}")));
        }
        println!("sample {address:#x}: equal");
    }
    let mut environment = [0u8; 4096];
    let mut environment_again = [0u8; 4096];
    read_range(&file, 0x1ff000, &mut environment)?;
    read_range(&file, 0x1ff000, &mut environment_again)?;
    if environment != environment_again {
        return Err(io::Error::other("unstable environment"));
    }
    let stored = u32::from_le_bytes(environment[..4].try_into().unwrap());
    let calculated = crc32(&environment[4..]);
    if stored != calculated {
        return Err(io::Error::other(format!(
            "environment CRC mismatch: {stored:08x} != {calculated:08x}"
        )));
    }
    println!("environment CRC: {stored:08x}");
    if probe_only {
        return Ok(());
    }

    let mut first = vec![0u8; SIZE];
    let mut second = vec![0u8; SIZE];
    for address in (0..SIZE).step_by(BLOCK) {
        read_block(&file, address, &mut first[address..address + BLOCK])?;
    }
    for address in (0..SIZE).step_by(BLOCK) {
        read_block(&file, address, &mut second[address..address + BLOCK])?;
        if first[address..address + BLOCK] != second[address..address + BLOCK] {
            return Err(io::Error::other(format!("ROM mismatch at {address:#x}")));
        }
    }
    if first.iter().all(|byte| *byte == 0xff) || first.iter().all(|byte| *byte == 0) {
        return Err(io::Error::other("empty ROM data"));
    }
    if first[0x1ff000..] != environment {
        return Err(io::Error::other("environment changed during read"));
    }
    let mut output = OpenOptions::new().write(true).create_new(true).open(OUTPUT)?;
    output.write_all(&first)?;
    output.sync_all()?;
    println!("saved {SIZE} bytes to {OUTPUT}; two full reads equal");
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 || (args[1] != "--probe" && args[1] != "--dump") {
        eprintln!("usage: spi_read --probe|--dump");
        std::process::exit(2);
    }
    if let Err(error) = run(args[1] == "--probe") {
        eprintln!("SPI read failed: {error}");
        std::process::exit(1);
    }
}
