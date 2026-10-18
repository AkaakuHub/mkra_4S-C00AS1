use crate::{channels::ChannelTable, stream::Broker};
use std::{
    io,
    os::unix::process::CommandExt,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub fn start(
    path: String,
    loaded: String,
    base: ChannelTable,
    broker: Arc<Broker>,
    cleanup_failed: Arc<AtomicBool>,
) -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    thread::Builder::new()
        .name("channel-sync".into())
        .spawn(move || {
            let mut rejected = None;
            loop {
                thread::sleep(Duration::from_secs(5));
                let text = match std::fs::read_to_string(&path) {
                    Ok(text) => text,
                    Err(error) => {
                        eprintln!("mkra: cannot read updated ISDB-S channel table: {error}");
                        continue;
                    }
                };
                if text == loaded || rejected.as_ref() == Some(&text) {
                    continue;
                }
                if let Err(error) = base.clone().add_isdbs_tsv(&text) {
                    eprintln!("mkra: updated ISDB-S channel table rejected: {error}");
                    rejected = Some(text);
                    continue;
                }
                let result = broker.run_when_idle(Duration::ZERO, || {
                    if cleanup_failed.load(Ordering::SeqCst) {
                        return Err(io::Error::other(
                            "previous cleanup failed; channel table reload is blocked",
                        ));
                    }
                    if std::fs::read_to_string(&path)? != text {
                        return Ok(());
                    }
                    eprintln!("mkra: applying updated ISDB-S channel table while idle");
                    Err(Command::new("/proc/self/exe")
                        .arg0(&args[0])
                        .args(&args[1..])
                        .exec())
                });
                if let Err(error) = result {
                    eprintln!("mkra: ISDB-S channel table reload failed: {error}");
                    rejected = Some(text);
                }
            }
        })
        .map(|_| ())
}
