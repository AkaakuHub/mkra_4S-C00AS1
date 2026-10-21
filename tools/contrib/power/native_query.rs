mod console_capture;

use std::ffi::{c_int, c_void};
use std::fs;
use std::io;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::process::Command;

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

fn receiver_queue() -> io::Result<c_int> {
    let processes: Vec<_> = fs::read_dir("/proc")?
        .filter_map(Result::ok)
        .filter(|entry| {
            fs::read_to_string(entry.path().join("comm"))
                .ok()
                .as_deref()
                == Some("abs\n")
        })
        .collect();
    if processes.len() != 1 {
        return Err(io::Error::other("expected one abs process"));
    }
    let process = processes[0].path();
    let hash = Command::new("/usr/bin/md5sum")
        .arg(process.join("exe"))
        .output()?;
    if !hash.status.success()
        || !String::from_utf8_lossy(&hash.stdout).starts_with("1a3fdfa8379d75a802190b48ea951e9a ")
    {
        return Err(io::Error::other("unexpected abs firmware"));
    }
    let queues: Vec<c_int> = fs::read_to_string("/tmp/bdh_out_msgh.lst")?
        .lines()
        .filter_map(|line| line.strip_prefix("443-"))
        .filter_map(|rest| rest.split_whitespace().next()?.parse().ok())
        .collect();
    if queues.len() != 1 {
        return Err(io::Error::other("expected one native debug queue"));
    }
    let queue = queues[0];
    let ipc = fs::read_to_string("/proc/sysvipc/msg")?;
    let rows: Vec<Vec<&str>> = ipc
        .lines()
        .skip(1)
        .map(|line| line.split_whitespace().collect())
        .filter(|fields: &Vec<&str>| {
            fields.get(1).and_then(|value| value.parse().ok()) == Some(queue)
        })
        .collect();
    if rows.len() != 1 || rows[0].get(3) != Some(&"0") || rows[0].get(4) != Some(&"0") {
        return Err(io::Error::other("native debug queue is busy or absent"));
    }
    let mut waiters = 0;
    for thread in fs::read_dir(process.join("task"))? {
        let syscall = fs::read_to_string(thread?.path().join("syscall"))?;
        if syscall.split_whitespace().nth(1) == Some(format!("{queue:#x}").as_str()) {
            waiters += 1;
        }
    }
    if waiters != 1 {
        return Err(io::Error::other(
            "expected one abs thread waiting on native debug queue",
        ));
    }
    Ok(queue)
}

fn query() -> io::Result<()> {
    if std::mem::size_of::<Wire>() != 96 {
        return Err(io::Error::other("requires ARM32 wire layout"));
    }
    let queue = receiver_queue()?;
    let mut wire = Wire {
        mtype: 1,
        unused: [0; 2],
        self_ptr: 0,
        inner: 0,
        msg_id: 0x80000,
        len: 56,
        src: 655,
        payload: [0; 64],
    };
    let command = b"SdmPvqsDbg Get 0 0";
    wire.payload[..4].copy_from_slice(&1_u32.to_le_bytes());
    wire.payload[4..4 + command.len()].copy_from_slice(command);
    let captured = console_capture::capture(|| {
        if unsafe { msgsnd(queue, &wire as *const Wire as *const c_void, 92, 0x800) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    })?;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open("/tmp/mt/native-query-console.log")?
        .write_all(&captured)?;
    let output = String::from_utf8_lossy(&captured);
    for line in output.lines().filter(|line| {
        line.contains("SdmPvqsDbg")
            || line.contains("usTimeToStep2=")
            || line.contains("usFlashMagic=")
    }) {
        println!("{line}");
    }
    if !output.contains("SdmPvqsDbg cmd=\"Get\",ret=1") {
        return Err(io::Error::other("native Get response was not captured"));
    }
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = if args.is_empty() {
        query()
    } else if args == ["--restore-console"] {
        console_capture::restore()
    } else {
        Err(io::Error::other("usage: native_query [--restore-console]"))
    };
    if let Err(error) = result {
        eprintln!("native power query: {error}");
        std::process::exit(1);
    }
}
