use std::fs;
use std::io;
use std::os::raw::{c_int, c_void};
use std::process::Command;

const FIRMWARE: &str = "1a3fdfa8379d75a802190b48ea951e9a";
const QID: &str = "123";
const EVENT: u32 = 0x11b64;
const IPC_NOWAIT: c_int = 0x800;

#[repr(C)]
struct Wire {
    mtype: i32,
    unused: [u32; 2],
    self_ptr: u32,
    inner: u32,
    msg_id: u32,
    len: u32,
    src: u32,
    payload: [u8; 64],
}

unsafe extern "C" {
    fn msgsnd(msqid: c_int, msgp: *const c_void, msgsz: usize, msgflg: c_int) -> c_int;
}

fn one_process(name: &str) -> io::Result<u32> {
    let mut matches = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry.file_name().to_string_lossy().parse::<u32>().ok() else {
            continue;
        };
        if fs::read_to_string(format!("/proc/{pid}/comm")).ok().as_deref() == Some(name) {
            matches.push(pid);
        }
    }
    if matches.len() != 1 {
        return Err(io::Error::other(format!("expected one {name} process")));
    }
    Ok(matches[0])
}

fn one_fpm(pid: u32) -> io::Result<u32> {
    let mut matches = Vec::new();
    for entry in fs::read_dir(format!("/proc/{pid}/task"))? {
        let entry = entry?;
        let Ok(tid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if fs::read_to_string(format!("/proc/{pid}/task/{tid}/comm"))
            .ok()
            .as_deref()
            == Some("abs.T_FPM\n")
        {
            matches.push(tid);
        }
    }
    if matches.len() != 1 {
        return Err(io::Error::other("expected one FPM thread"));
    }
    Ok(matches[0])
}

fn run() -> io::Result<()> {
    let pid = one_process("abs\n")?;
    let hash = Command::new("/usr/bin/md5sum")
        .arg(format!("/proc/{pid}/exe"))
        .output()?;
    if !hash.status.success() || !String::from_utf8_lossy(&hash.stdout).starts_with(FIRMWARE) {
        return Err(io::Error::other("unexpected abs binary"));
    }
    let fpm = one_fpm(pid)?;
    let map = fs::read_to_string("/tmp/bdh_out_msgh.lst")?;
    let queues: Vec<_> = map
        .lines()
        .filter_map(|line| line.strip_prefix(&format!("{QID}-")))
        .filter_map(|rest| rest.split_whitespace().next()?.parse::<c_int>().ok())
        .collect();
    if queues.len() != 1 {
        return Err(io::Error::other("expected one FPM queue"));
    }
    let msqid = queues[0];
    let sysvipc = fs::read_to_string("/proc/sysvipc/msg")?;
    let rows: Vec<Vec<&str>> = sysvipc
        .lines()
        .skip(1)
        .map(|line| line.split_whitespace().collect())
        .filter(|fields: &Vec<&str>| fields.get(1).and_then(|id| id.parse::<c_int>().ok()) == Some(msqid))
        .collect();
    if rows.len() != 1
        || rows[0].get(3) != Some(&"0")
        || rows[0].get(4) != Some(&"0")
        || rows[0].get(6).and_then(|tid| tid.parse::<u32>().ok()) != Some(fpm)
    {
        return Err(io::Error::other("FPM queue busy or wrong receiver"));
    }
    let meminfo = fs::read_to_string("/proc/meminfo")?;
    let free = meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemFree:")?.split_whitespace().next()?.parse::<u32>().ok())
        .ok_or_else(|| io::Error::other("MemFree unavailable"))?;
    if free <= 100_000 {
        return Err(io::Error::other("insufficient free memory"));
    }
    let wire = Wire {
        mtype: 1,
        unused: [0; 2],
        self_ptr: 0,
        inner: 0,
        msg_id: EVENT,
        len: 0,
        src: 1,
        payload: [0; 64],
    };
    if std::mem::size_of::<Wire>() != 96 {
        return Err(io::Error::other("unexpected wire size"));
    }
    let rc = unsafe { msgsnd(msqid, &wire as *const Wire as *const c_void, 92, IPC_NOWAIT) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    println!("sent msgIniRequestFpmPowerOnOStart to qid {QID}, msqid {msqid}");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("FPM wake: {error}");
        std::process::exit(1);
    }
}
