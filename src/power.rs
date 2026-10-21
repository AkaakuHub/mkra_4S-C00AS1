use crate::{gr_i2c, stream::Broker};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

mod lan;

pub fn start_lan_idle_task(
    broker: Arc<Broker>,
    delay: Duration,
    cleanup_failed: Arc<AtomicBool>,
) -> io::Result<()> {
    let device = lan::Lan::open()?;
    thread::Builder::new()
        .name("mkra-lan-idle".into())
        .spawn(move || {
            let mut mac_asleep = false;
            let mut mac_wake_at = Instant::now();
            loop {
                thread::sleep(Duration::from_secs(1));
                if cleanup_failed.load(Ordering::SeqCst) {
                    continue;
                }
                if mac_asleep {
                    match device.software_power_down() {
                        Ok(true) => continue,
                        Ok(false) => {
                            mac_asleep = false;
                            mac_wake_at = Instant::now();
                            eprintln!("mkra: LAN restored by wake packet");
                        }
                        Err(error) => {
                            eprintln!("mkra: cannot read LAN power state: {error}");
                            continue;
                        }
                    }
                }
                if mac_wake_at.elapsed() < delay {
                    continue;
                }
                let result = broker.run_when_idle(delay, || {
                    if device.clients_connected()? {
                        return Ok(());
                    }
                    device.standby()?;
                    mac_asleep = true;
                    eprintln!("mkra: LAN entered magic-packet standby");
                    Ok(())
                });
                if let Err(e) = result {
                    mac_wake_at = Instant::now();
                    eprintln!("mkra: idle LAN standby failed: {e}");
                }
            }
        })
        .map(|_| ())
}

pub fn status() -> io::Result<()> {
    let registers = gr_i2c::satellite_rf_registers()?;
    println!(
        "Satellite RF standby snapshot match: {}",
        registers == gr_i2c::SATELLITE_STANDBY_REGS
    );
    for (reg, value) in registers {
        println!("  RF[0x{reg:02x}]=0x{value:02x}");
    }
    Ok(())
}

pub fn is_sleeping() -> io::Result<bool> {
    Ok(gr_i2c::satellite_rf_registers()? == gr_i2c::SATELLITE_STANDBY_REGS)
}

