use std::io;

use crate::graph::{ksym, Kmem};

pub fn print() -> io::Result<()> {
    let mut kmem = Kmem::open()?;
    let ctx = kmem.u32(ksym("gps_XCode")?)?;
    let sg_av = kmem.u32(ksym("gncp_sg_av")?)?;
    if ctx < 0x8000_0000 || sg_av < 0x8000_0000 {
        return Err(io::Error::other("invalid XCode6 firmware status pointers"));
    }

    let state = kmem.u32(ctx + 0x46398)?;
    let tracked_heartbeat = kmem.u32(ctx + 0x4639c)?;
    let stale_count = kmem.u32(ctx + 0x463a0)?;
    let last_check = kmem.u32(ctx + 0x463a4)?;
    println!("XCode6 firmware status (read-only):");
    println!("  ctx={ctx:#010x} sg_av={sg_av:#010x}");
    println!("  get_fw_state flag={state} stale_count={stale_count}");
    println!("  heartbeat tracked={tracked_heartbeat:#010x}");
    println!("  last_check={last_check:#010x}");
    Ok(())
}
