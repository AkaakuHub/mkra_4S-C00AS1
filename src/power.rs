use crate::{fw_state, gr_i2c, snapshot, stream::Broker, tuner, xcdrv::XcDrv};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub fn start_idle_task(
    broker: Arc<Broker>,
    delay: Duration,
    resume_required: Arc<AtomicBool>,
    cleanup_failed: Arc<AtomicBool>,
) -> io::Result<()> {
    thread::Builder::new()
        .name("mkra-idle".into())
        .spawn(move || loop {
            thread::sleep(Duration::from_secs(1));
            if resume_required.load(Ordering::SeqCst) || cleanup_failed.load(Ordering::SeqCst) {
                continue;
            }
            let result = broker.run_when_idle(delay, || {
                resume_required.store(true, Ordering::SeqCst);
                sleep_receiver()
            });
            if let Err(e) = result {
                eprintln!("mkra: idle receiver sleep failed: {e}; next stream will retune");
            }
        })
        .map(|_| ())
}

pub fn status() -> io::Result<()> {
    let registers = gr_i2c::satellite_rf_registers()?;
    println!(
        "Satellite RF standby: {}",
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

pub fn sleep_receiver() -> io::Result<()> {
    let pid = snapshot::procmem::find_pid("tuner")?;
    snapshot::verify_tuner_binary(pid)?;
    let drv = XcDrv::open_path(crate::xcdrv::DEV_PATH)?;
    fw_state::ensure_p4_idle(&drv)?;
    let request = tuner::FedPowerOffRequest::isdb_s(1, 2)
        .ok_or_else(|| io::Error::other("invalid satellite power-off path"))?;
    tuner::Tuner::open_qid(request.fed_qid, tuner::MSGH_LIST)?.power_off_fed(&request)?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if gr_i2c::satellite_rf_registers()? == gr_i2c::SATELLITE_STANDBY_REGS {
            eprintln!("mkra: satellite RF standby confirmed; receiver API remains available");
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::other("satellite RF standby was not confirmed"));
        }
        thread::sleep(Duration::from_millis(100));
    }
}
