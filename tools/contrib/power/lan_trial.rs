use crate::lan::Lan;
use std::io;
use std::sync::mpsc::sync_channel;
use std::thread;
use std::time::{Duration, Instant};

pub fn run(lan: &Lan) -> io::Result<()> {
    if lan.pmt()? & 1 != 0 || lan.software_power_down()? {
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
                let restored = lan.restore()?;
                eprintln!(
                    "deadline_ms={} software_restore={restored}",
                    started.elapsed().as_millis()
                );
                Ok(())
            })?;
        receive_ready
            .recv()
            .map_err(|_| io::Error::other("restoration thread did not start"))?;
        lan.standby()?;
        let armed = lan.pmt()?;
        eprintln!(
            "armed_ms={} pmt={armed:#010x}",
            started.elapsed().as_millis()
        );
        if armed & 3 != 3 || !lan.software_power_down()? {
            return Err(io::Error::other("MAC did not enter magic-packet standby"));
        }
        guardian
            .join()
            .map_err(|_| io::Error::other("restoration thread panicked"))??;
        if lan.software_power_down()? || lan.pmt()? & 1 != 0 {
            return Err(io::Error::other("MAC is still in standby after the test"));
        }
        println!("bounded MAC test finished; Linux was kept running");
        Ok(())
    })
}
