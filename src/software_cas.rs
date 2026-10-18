use crate::cas::EcmRejected;
use std::{
    io,
    sync::atomic::{AtomicU64, Ordering},
};

static ACCEPTED: AtomicU64 = AtomicU64::new(0);

unsafe extern "C" {
    fn bcas_decodeECM(payload: *const u8, size: u32, keys: *mut u8, variable: *mut u8) -> i32;
}

pub fn ecm(payload: &[u8]) -> io::Result<[[u8; 8]; 2]> {
    let size = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "ECM payload is too large"))?;
    let mut keys = [[0u8; 8]; 2];
    let result = unsafe {
        bcas_decodeECM(
            payload.as_ptr(),
            size,
            keys.as_mut_ptr().cast(),
            std::ptr::null_mut(),
        )
    };
    if result != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            EcmRejected::Software(result),
        ));
    }
    ACCEPTED.fetch_add(1, Ordering::Relaxed);
    Ok(keys)
}

pub fn accepted_updates() -> u64 {
    ACCEPTED.load(Ordering::Relaxed)
}
