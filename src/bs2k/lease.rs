use std::{
    fs, io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

const HELD: &str = "/sys/module/as1_tuner/parameters/held";

pub fn reset() -> io::Result<()> {
    fs::write(HELD, b"0")
}

pub struct Lease {
    dirty: Arc<AtomicBool>,
}

impl Lease {
    pub fn acquire(dirty: Arc<AtomicBool>) -> io::Result<Self> {
        fs::write(HELD, b"1")?;
        let lease = Self { dirty };
        crate::native_selection::Reader::open()?.wait_idle(Duration::from_secs(90))?;
        Ok(lease)
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if self.dirty.load(Ordering::SeqCst) {
            return;
        }
        if let Err(error) = reset() {
            self.dirty.store(true, Ordering::SeqCst);
            eprintln!("mkra: releasing BS2K tuner ownership failed: {error}");
        }
    }
}
