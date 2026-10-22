//! Owned live ISDB-S graph. The shared Broker serializes this with BS4K.
mod capture;
mod lease;

use capture::Capture;
use crate::{
    cas::NativeCard,
    descramble::Decoder,
    gr_i2c, snapshot,
    tssrc::{TsSource, XcReaderSource},
    tuner,
    xcdrv::{TsiMappingSnapshot, XcDrv},
};
use std::{
    io,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
fn err(s: impl Into<String>) -> io::Error {
    io::Error::other(s.into())
}
fn identity() -> io::Result<()> {
    for (path, want) in [
        ("/usr/bin/tuner", "b94fc6a8f2e506dc66336c5d0d031b69"),
        ("/usr/bin/abs", "1a3fdfa8379d75a802190b48ea951e9a"),
        ("/disk/xc6drv/xcdrv.ko", "0bfa0f7db130e058f3ec838bbd681b7d"),
        (
            "/usr/vixs/lib/libvixs.so",
            "344f0ef7b1a84d49ae13659d3a388d78",
        ),
    ] {
        let r = Command::new("md5sum").arg(path).output()?;
        if !r.status.success()
            || !String::from_utf8_lossy(&r.stdout).starts_with(&format!("{want} "))
        {
            return Err(err(format!("unverified OEM image: {path}")));
        }
    }
    Ok(())
}
/// Recover the current frontend mode on server restart, without retuning.
/// tuner:aDevOfPath[1] is the FEM's real path1 assignment, not cached SI.
pub fn legacy_frontend() -> io::Result<bool> {
    identity()?;
    lease::reset()?;
    let pid = snapshot::procmem::find_pid("tuner")?;
    snapshot::procmem::verify_layout(pid)?;
    let bytes = snapshot::procmem::read(pid, tuner::A_DEV_OF_PATH_ADDR + 4, 4)?;
    match u32::from_le_bytes(bytes.try_into().unwrap()) {
        2 => Ok(true),
        0x200 | 1 => Ok(false),
        _ => Err(err("unexpected FEM path1 assignment")),
    }
}

pub struct Source {
    drv: Arc<XcDrv>,
    mapping: Option<TsiMappingSnapshot>,
    pipeline: u32,
    av: u32,
    device: u32,
    buffer: u32,
    connection: u32,
    filter: bool,
    running: bool,
    capture: Option<Capture>,
    decoder: Decoder<NativeCard>,
    last_data: Instant,
    decode_max_us: u128,
    dirty: Arc<AtomicBool>,
    label: String,
    _lease: lease::Lease,
}
impl Source {
    pub fn open(
        drv: Arc<XcDrv>,
        freq: u32,
        tsid: u16,
        legacy: &AtomicBool,
        dirty: Arc<AtomicBool>,
    ) -> io::Result<Self> {
        if dirty.load(Ordering::SeqCst) {
            return Err(err(
                "previous graph cleanup failed; inspect before restarting",
            ));
        }
        identity()?;
        let lease = lease::Lease::acquire(Arc::clone(&dirty))?;
        let card = NativeCard::open()?;
        let (system, iv) = card.parameters()?;
        crate::fw_state::ensure_p4_idle(&drv)?;
        let mapping = drv.snapshot_tsi_mapping()?;
        let mut source = Self {
            drv,
            mapping: Some(mapping),
            pipeline: 0,
            av: 0,
            device: 0,
            buffer: 0,
            connection: 0,
            filter: false,
            running: false,
            capture: None,
            decoder: Decoder::new(card, system, iv, tsid).full_multiplex(),
            last_data: Instant::now(),
            decode_max_us: 0,
            dirty,
            label: format!("ISDB-S RF {freq} TSID {tsid:04X} / native CAS + MULTI2"),
            _lease: lease,
        };
        let req = tuner::FedTuneRequest::isdb_s(1, tsid, freq)
            .ok_or_else(|| err("invalid ISDB-S path"))?;
        req.validate().map_err(err)?;
        legacy.store(true, Ordering::SeqCst); // includes partial frontend changes
        tuner::Tuner::open(tuner::MSGH_LIST)?.set_device(&tuner::FemDeviceRequest::assign(1, 2))?;
        std::thread::sleep(Duration::from_millis(300));
        tuner::Tuner::open_qid(req.fed_qid, tuner::MSGH_LIST)?.tune_fed(&req)?;
        let until = Instant::now() + Duration::from_secs(12);
        loop {
            let r = gr_i2c::read_output_regs()?;
            if r.lock & 0x20 != 0
                && r.bank2_selection[..2] == tsid.to_be_bytes()
                && r.bank0.iter().any(|&(r, v)| r == 0xcc && v & 0x80 != 0)
                && r.bank7.iter().any(|&(r, v)| r == 0x72 && v == 0)
            {
                match gr_i2c::make_isdbs_ts_output_exclusive() {
                    Ok(_) => break,
                    Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                        if Instant::now() >= until {
                            return Err(error);
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
            if Instant::now() >= until {
                return Err(err(
                    "ISDB-S lock/output/TSID did not match requested channel",
                ));
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        source.pipeline = source.drv.create_pipeline()?;
        source.av = source.drv.create_av_sync()?;
        source.drv.set_pipeline(source.av, source.pipeline)?;
        source.drv.configure_product_mpeg_ts_tsi(0)?;
        source.drv.map_tsi_one_to_zero()?;
        source.drv.set_av_sync_live_mode(source.av)?;
        source.device = source.drv.create_mpeg_ts_demux()?;
        source.drv.set_pipeline(source.device, source.pipeline)?;
        source.drv.config_ts_recorder_tsi(source.device, 1)?;
        source.drv.run_product_demux(source.device)?;
        source.running = true;
        source.buffer = source.drv.create_demux_ts_buffer()?;
        source.connection = source.drv.connect_output(source.device, source.buffer)?;
        source.capture = Some(Capture::start(XcReaderSource::attach(
            Arc::clone(&source.drv),
            source.buffer,
            capture::READ_TIMEOUT_MS,
        )?)?);
        source
            .drv
            .add_demux_pid(source.device, u32::MAX, source.connection)?;
        source.filter = true;
        source.drv.set_buffer_ready(source.buffer)?;
        source.last_data = Instant::now();
        eprintln!("mkra: opened {}", source.label);
        Ok(source)
    }
}
impl TsSource for Source {
    fn read_into(&mut self, out: &mut Vec<u8>) -> io::Result<usize> {
        let before = out.len();
        let bytes = self.capture.as_mut().unwrap().read()?;
        let result = (|| {
            if !bytes.is_empty() {
                self.last_data = Instant::now();
                let started = Instant::now();
                let decoded = self.decoder.push(&bytes, out);
                self.decode_max_us = self.decode_max_us.max(started.elapsed().as_micros());
                decoded?;
            } else if self.last_data.elapsed() > Duration::from_secs(8) {
                return Err(err("BS2K input stopped for eight seconds"));
            }
            Ok(out.len() - before)
        })();
        self.capture.as_ref().unwrap().recycle(bytes);
        result.map_err(|e| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("BS2K session ended: {e}"),
            )
        })
    }
    fn describe(&self) -> String {
        self.label.clone()
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        let mut good = true;
        let mut check = |label: &str, r: io::Result<()>| {
            if let Err(e) = r {
                good = false;
                eprintln!("mkra: BS2K cleanup {label}: {e}");
            }
        };
        if self.filter {
            check(
                "remove PID",
                self.drv.remove_demux_pid(self.device, u32::MAX),
            );
        }
        drop(self.capture.take());
        if self.connection != 0 {
            if let Err(first) = self
                .drv
                .disconnect_product_demux(self.device, self.connection)
            {
                eprintln!("mkra: BS2K disconnect needs stop/retry: {first}");
                if self.running {
                    check("stop retry", self.drv.stop_device(self.device));
                    self.running = false;
                }
                check(
                    "disconnect retry",
                    self.drv
                        .disconnect_product_demux(self.device, self.connection),
                );
            }
        }
        let clear = self.buffer == 0
            || self
                .drv
                .sbuf_status_raw(self.buffer)
                .is_ok_and(|s| s[8] == 0 && s[9] == 0);
        if self.buffer != 0 {
            if clear {
                check("reset ring", self.drv.force_reset_buffer(self.buffer));
                check("destroy ring", self.drv.destroy_buffer(self.buffer));
            } else {
                check(
                    "ring ownership",
                    Err(err("retaining graph with live writer/readers")),
                );
            }
        }
        if self.running {
            check("stop", self.drv.stop_device(self.device));
        }
        if self.device != 0 && clear {
            check("destroy DEMUX", self.drv.destroy_device(self.device));
        }
        if self.pipeline != 0 && clear {
            check("stop pipeline", self.drv.stop_pipeline(self.pipeline));
        }
        if self.av != 0 && clear {
            check("destroy AV_SYNC", self.drv.destroy_device(self.av));
        }
        if self.pipeline != 0 && clear {
            check("destroy pipeline", self.drv.destroy_pipeline(self.pipeline));
        }
        if clear {
            if let Some(mapping) = self.mapping.take() {
                check("restore TSI", self.drv.restore_tsi_mapping(&mapping));
            }
        }
        if !good {
            self.dirty.store(true, Ordering::SeqCst);
        }
        if !self.decoder.encrypted_passthrough.is_empty() {
            eprintln!(
                "mkra: unassociated encrypted PIDs retained with scrambling flags: {:?}",
                self.decoder.encrypted_passthrough
            );
        }
        eprintln!(
            "mkra: BS2K closed: {} TS packets / {} decrypted / {} ECM updates; cleanup_ok={good} decode_max_us={} ecm_time_us={} ecm_max_us={}",
            self.decoder.packets, self.decoder.decrypted, self.decoder.updates,
            self.decode_max_us, self.decoder.ecm_time_us, self.decoder.ecm_max_us,
        );
        #[cfg(feature = "software-cas")]
        eprintln!(
            "mkra: software ECM updates accepted by this process: {}",
            crate::software_cas::accepted_updates()
        );
    }
}
/// Restore native frontend initialization. Actual 4K service tune follows in factory.
pub fn restore_frontend() -> io::Result<()> {
    identity()?;
    gr_i2c::quiesce_ts_output()?;
    tuner::Tuner::open(tuner::MSGH_LIST)?.set_device(&tuner::FemDeviceRequest::assign(1, 0x200))?;
    std::thread::sleep(Duration::from_millis(300));
    let pid = snapshot::procmem::find_pid("tuner")?;
    snapshot::procmem::verify_layout(pid)?;
    for addr in [snapshot::RECOMMEND_FLAGS, snapshot::PSEQ_FLAGS] {
        let bytes = snapshot::procmem::read(pid, addr, 4)?;
        match u32::from_le_bytes(bytes.try_into().unwrap()) {
            0 => {}
            1 => {
                snapshot::oneshot::clear(pid, addr)?;
            }
            _ => return Err(err("unexpected tuner initialization flag")),
        }
    }
    Ok(())
}
