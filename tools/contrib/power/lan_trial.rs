use crate::{pmt, request, software_power_down};
use std::io;
use std::os::fd::OwnedFd;
use std::sync::mpsc::sync_channel;
use std::thread;
use std::time::{Duration, Instant};

fn restore(fd: &OwnedFd) -> io::Result<bool> {
    if !software_power_down(fd)? {
        return Ok(false);
    }
    if pmt(fd)? & 1 == 0 {
        request(fd, 0x89f6, &mut [1])?;
    }
    request(fd, 0x89f6, &mut [2])?;
    if software_power_down(fd)? || pmt(fd)? & 1 != 0 {
        return Err(io::Error::other("MAC restoration did not complete"));
    }
    Ok(true)
}

pub fn run(fd: &OwnedFd) -> io::Result<()> {
    if pmt(fd)? & 1 != 0 || software_power_down(fd)? {
        return Err(io::Error::other(
            "MAC must be awake before the bounded test",
        ));
    }
    let started = Instant::now();
    thread::scope(|scope| {
        let (ready, receive_ready) = sync_channel(0);
        let guardian = thread::Builder::new()
            .name("lan-restore".into())
            .spawn_scoped(scope, move || -> io::Result<()> {
                ready
                    .send(())
                    .map_err(|_| io::Error::other("test controller stopped"))?;
                thread::sleep(Duration::from_secs(15));
                let restored = restore(fd)?;
                eprintln!(
                    "deadline_ms={} software_restore={restored}",
                    started.elapsed().as_millis()
                );
                Ok(())
            })?;
        receive_ready
            .recv()
            .map_err(|_| io::Error::other("restoration thread did not start"))?;
        request(fd, 0x89f6, &mut [1])?;
        let armed = pmt(fd)?;
        eprintln!(
            "armed_ms={} pmt={armed:#010x}",
            started.elapsed().as_millis()
        );
        if armed & 3 != 3 || !software_power_down(fd)? {
            return Err(io::Error::other("MAC did not enter magic-packet standby"));
        }
        guardian
            .join()
            .map_err(|_| io::Error::other("restoration thread panicked"))??;
        if software_power_down(fd)? || pmt(fd)? & 1 != 0 {
            return Err(io::Error::other("MAC is still in standby after the test"));
        }
        println!("bounded MAC test finished; Linux was kept running");
        Ok(())
    })
}
