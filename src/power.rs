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

pub fn start_idle_task(
    broker: Arc<Broker>,
    delay: Duration,
    cleanup_failed: Arc<AtomicBool>,
) -> io::Result<()> {
    let device = lan::Lan::open()?;
    if std::fs::read_to_string("/sys/module/as1_freezer/parameters/enabled")?.trim() != "1" {
        return Err(io::Error::other("as1_freezer is disabled"));
    }
    for module in ["display", "wake", "lan", "front", "mmc", "guard"] {
        let stats = std::fs::read_to_string(format!(
            "/sys/module/as1_{module}/parameters/stats"
        ))?;
        if !stats.split_whitespace().any(|field| field == "enabled=1") {
            return Err(io::Error::other(format!("as1_{module} is disabled")));
        }
    }
    if std::fs::read_to_string("/sys/power/pm_test")?.trim()
        != "[none] core processors platform devices freezer"
    {
        return Err(io::Error::other("power diagnostics are enabled"));
    }
    if std::fs::read_to_string("/sys/module/printk/parameters/console_suspend")?.trim() != "N" {
        return Err(io::Error::other("native MCU UART must stay enabled"));
    }
    std::fs::write("/sys/module/as1_codec/parameters/bypass", b"1")?;
    thread::Builder::new()
        .name("mkra-wol-idle".into())
        .spawn(move || {
            let mut wake_at = Instant::now();
            loop {
                thread::sleep(Duration::from_secs(1));
                if cleanup_failed.load(Ordering::SeqCst) {
                    continue;
                }
                if wake_at.elapsed() < delay {
                    continue;
                }
                let result = broker.run_when_idle(delay, || {
                    if device.clients_connected()? {
                        return Ok(());
                    }
                    eprintln!("mkra: entering body standby; waiting for magic packet");
                    let result = std::fs::write("/sys/power/state", b"freeze");
                    wake_at = Instant::now();
                    result?;
                    eprintln!("mkra: body resumed from standby");
                    Ok(())
                });
                if let Err(e) = result {
                    wake_at = Instant::now();
                    eprintln!("mkra: idle body standby failed: {e}");
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

