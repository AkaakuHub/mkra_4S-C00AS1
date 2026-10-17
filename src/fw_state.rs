use std::io;

use crate::graph::{ksym, Kmem};
use crate::xcdrv::{XcDrv, DEV_PATH};

pub fn ensure_p4_idle(drv: &XcDrv) -> io::Result<()> {
    let mut kmem = Kmem::open()?;
    let ctx = kmem.u32(ksym("gps_XCode")?)?;
    if ctx < 0x8000_0000 || kmem.u32(ctx + 0x463b0)? != 100 {
        return Err(io::Error::other("unexpected XCode6 request queue layout"));
    }
    let free_head = ctx + 0x48014;
    if kmem.u32(ctx + 0x48020)? != free_head {
        return Err(io::Error::other("unexpected XCode6 request queue head"));
    }
    if kmem.u32(free_head)? == free_head {
        return Err(io::Error::other("XCode6 request queue has no free slots"));
    }
    let pending = drv.mmr_read32(0x170d4)?;
    if pending != 0 {
        return Err(io::Error::other(format!("P4 command notification is pending: 0x{pending:08x}")));
    }
    Ok(())
}

pub fn print() -> io::Result<()> {
    let mut kmem = Kmem::open()?;
    let ctx = kmem.u32(ksym("gps_XCode")?)?;
    let sg_av = kmem.u32(ksym("gncp_sg_av")?)?;
    if ctx < 0x8000_0000 || sg_av < 0x8000_0000 {
        return Err(io::Error::other("invalid XCode6 firmware status pointers"));
    }

    let state = kmem.u32(ctx + 0x46398)?;
    let mmr_base = kmem.u32(ctx)?;
    let mmr_phys = kmem.u32(ctx + 0x20)?;
    let mmr_len = kmem.u32(ctx + 0x24)?;
    let tracked_heartbeat = kmem.u32(ctx + 0x4639c)?;
    let stale_count = kmem.u32(ctx + 0x463a0)?;
    let last_check = kmem.u32(ctx + 0x463a4)?;
    let buffer_queue_capacity = kmem.u32(ctx + 0x463b0)?;
    let free_head = kmem.u32(ctx + 0x48020)?;
    if free_head != ctx + 0x48014 {
        return Err(io::Error::other("unexpected firmware queue free-list head"));
    }
    let free_next = kmem.u32(free_head)?;
    let dispatch_flags = kmem.read(ctx + 0x48b90, 16 * 4)?;
    let pending_dispatch: Vec<_> = dispatch_flags
        .chunks_exact(4)
        .enumerate()
        .filter_map(|(core, bytes)| {
            let flag = u32::from_le_bytes(bytes.try_into().ok()?);
            (flag != 0).then_some((core, flag))
        })
        .collect();
    println!("XCode6 firmware status (read-only):");
    println!("  ctx={ctx:#010x} mmr_base={mmr_base:#010x} sg_av={sg_av:#010x}");
    println!("  get_fw_state flag={state} stale_count={stale_count}");
    println!("  heartbeat tracked={tracked_heartbeat:#010x}");
    println!("  last_check={last_check:#010x}");
    println!("  buffer_queue_capacity={buffer_queue_capacity}");
    println!("  buffer_queue_free_head={free_head:#010x} next={free_next:#010x}");
    println!("  buffer_queue_free_list_empty={}", free_next == free_head);
    println!("  ipc_pending_dispatch={pending_dispatch:?}");
    if mmr_base != 0xffee_0000 || mmr_phys != mmr_base || mmr_len != 0x20000 {
        return Err(io::Error::other("unexpected XCode6 MMR mapping"));
    }
    let drv = XcDrv::open_path(DEV_PATH)?;
    let ipc_cmd = drv.mmr_read32(0x17104)?;
    let ipc_ack = drv.mmr_read32(0x1710c)?;
    let p4_cmd = drv.mmr_read32(0x170d4)?;
    let p4_ack = drv.mmr_read32(0x170dc)?;
    println!("  ipc_command_bits={ipc_cmd:#010x} ipc_ack_bits={ipc_ack:#010x}");
    println!("  p4_ipc_command_bits={p4_cmd:#010x} p4_ipc_ack_bits={p4_ack:#010x}");
    Ok(())
}

pub fn print_queue() -> io::Result<()> {
    let mut kmem = Kmem::open()?;
    let ctx = kmem.u32(ksym("gps_XCode")?)?;
    if ctx < 0x8000_0000 || kmem.u32(ctx + 0x463b0)? != 100 {
        return Err(io::Error::other("unexpected XCode6 diagnostic queue layout"));
    }
    let data = kmem.read(ctx + 0x463b8, 100 * 0x3c)?;
    if data.len() != 100 * 0x3c {
        return Err(io::Error::other("short XCode6 diagnostic queue read"));
    }
    println!("XCode6 diagnostic slots (read-only):");
    println!("index allocated state command subcommand processor device_handle");
    for index in 0..100 {
        let base = index * 0x3c;
        let word = |offset: usize| {
            let p = base + offset;
            u32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]])
        };
        println!(
            "{index:02} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x}",
            word(0), word(0x1c), word(0x20), word(0x24), word(0x34), word(0x38)
        );
    }
    Ok(())
}
