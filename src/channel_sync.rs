use crate::{channels::ChannelTable, stream::Broker};
#[cfg(feature = "software-cas")]
use std::os::unix::fs::MetadataExt;
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

pub struct ChannelFile {
    pub path: String,
    pub loaded: String,
    pub base: ChannelTable,
}

#[derive(PartialEq)]
struct Candidate {
    channels: Option<String>,
    #[cfg(feature = "software-cas")]
    keys: String,
}

#[cfg(feature = "software-cas")]
const KEY_GENERATION_ENV: &str = "MKRA_SOFTWARE_CAS_GENERATION";

#[cfg(feature = "software-cas")]
fn key_generation() -> io::Result<String> {
    let path = std::env::var_os("BCAS_KEYS_FILE")
        .ok_or_else(|| io::Error::other("BCAS_KEYS_FILE is not configured"))?;
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::other("BCAS_KEYS_FILE is not a regular file"));
    }
    Ok(format!(
        "{}:{}:{}:{}:{}:{}:{}",
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    ))
}

pub fn start(
    channels: Option<ChannelFile>,
    broker: Arc<Broker>,
    cleanup_failed: Arc<AtomicBool>,
) -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    #[cfg(feature = "software-cas")]
    let loaded_keys = std::env::var(KEY_GENERATION_ENV).ok();
    thread::Builder::new()
        .name("channel-sync".into())
        .spawn(move || {
            let mut rejected = None;
            loop {
                thread::sleep(Duration::from_secs(5));
                let text = match channels
                    .as_ref()
                    .map(|file| std::fs::read_to_string(&file.path))
                    .transpose()
                {
                    Ok(text) => text,
                    Err(error) => {
                        eprintln!("mkra: cannot read updated ISDB-S channel table: {error}");
                        continue;
                    }
                };
                #[cfg(feature = "software-cas")]
                let keys = match key_generation() {
                    Ok(keys) => keys,
                    Err(error) => {
                        eprintln!("mkra: cannot inspect software CAS configuration: {error}");
                        continue;
                    }
                };
                let candidate = Candidate {
                    channels: text,
                    #[cfg(feature = "software-cas")]
                    keys,
                };
                let changed = channels
                    .as_ref()
                    .is_some_and(|file| candidate.channels.as_ref() != Some(&file.loaded));
                #[cfg(feature = "software-cas")]
                let changed = changed || loaded_keys.as_ref() != Some(&candidate.keys);
                if !changed || rejected.as_ref() == Some(&candidate) {
                    continue;
                }
                if let (Some(file), Some(text)) = (&channels, &candidate.channels) {
                    if let Err(error) = file.base.clone().add_isdbs_tsv(text) {
                        eprintln!("mkra: updated ISDB-S channel table rejected: {error}");
                        rejected = Some(candidate);
                        continue;
                    }
                }
                let result = broker.run_when_idle(Duration::ZERO, || {
                    if cleanup_failed.load(Ordering::SeqCst) {
                        return Err(io::Error::other(
                            "previous cleanup failed; receiver configuration reload is blocked",
                        ));
                    }
                    if channels
                        .as_ref()
                        .map(|file| std::fs::read_to_string(&file.path))
                        .transpose()?
                        != candidate.channels
                    {
                        return Ok(());
                    }
                    #[cfg(feature = "software-cas")]
                    if key_generation()? != candidate.keys {
                        return Ok(());
                    }
                    let mut command = Command::new("/proc/self/exe");
                    command.arg0(&args[0]).args(&args[1..]);
                    #[cfg(feature = "software-cas")]
                    command.env(KEY_GENERATION_ENV, &candidate.keys);
                    eprintln!("mkra: applying receiver configuration while idle");
                    Err(command.exec())
                });
                if let Err(error) = result {
                    eprintln!("mkra: receiver configuration reload failed: {error}");
                    rejected = Some(candidate);
                }
            }
        })
        .map(|_| ())
}
