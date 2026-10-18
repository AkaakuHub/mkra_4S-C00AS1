use crate::tssrc::{TsSource, XcReaderSource};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender, TrySendError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

const BLOCK_BYTES: usize = 188 * 1024;
const QUEUE_BLOCKS: usize = 32;

pub(super) struct Capture {
    blocks: Receiver<Vec<u8>>,
    recycle: SyncSender<Vec<u8>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
    last_read: Option<Instant>,
    max_consumer_gap_us: Arc<AtomicU64>,
}

impl Capture {
    pub(super) fn start(reader: XcReaderSource) -> io::Result<Self> {
        let (sender, blocks) = sync_channel(QUEUE_BLOCKS);
        let (recycle, free) = sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let max_consumer_gap_us = Arc::new(AtomicU64::new(0));
        let worker_gap = Arc::clone(&max_consumer_gap_us);
        let worker = thread::Builder::new()
            .name("bs2k-capture".into())
            .spawn(move || run(reader, sender, free, &worker_stop, &worker_gap))?;
        Ok(Self {
            blocks,
            recycle,
            stop,
            worker: Some(worker),
            last_read: None,
            max_consumer_gap_us,
        })
    }

    pub(super) fn recycle(&self, mut block: Vec<u8>) {
        if !block.is_empty() {
            block.clear();
            let _ = self.recycle.try_send(block);
        }
    }

    pub(super) fn read(&mut self) -> io::Result<Vec<u8>> {
        if let Some(last) = self.last_read.take() {
            self.max_consumer_gap_us
                .fetch_max(last.elapsed().as_micros() as u64, Ordering::Relaxed);
        }
        match self.blocks.recv() {
            Ok(block) => {
                self.last_read = Some(Instant::now());
                Ok(block)
            }
            Err(_) => {
                self.join()?;
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "BS2K capture worker stopped",
                ))
            }
        }
    }

    fn join(&mut self) -> io::Result<()> {
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("BS2K capture worker panicked"))??;
        }
        Ok(())
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Err(error) = self.join() {
            eprintln!("mkra: BS2K capture ended: {error}");
        }
    }
}

fn enqueue(
    sender: &SyncSender<Vec<u8>>,
    free: &Receiver<Vec<u8>>,
    block: Vec<u8>,
    dropped_packets: &mut u64,
    short_blocks: &mut u64,
) -> Option<Vec<u8>> {
    let short = block.len() < BLOCK_BYTES;
    match sender.try_send(block) {
        Ok(()) => {
            *short_blocks += u64::from(short);
            Some(
                free.try_recv()
                    .unwrap_or_else(|_| Vec::with_capacity(BLOCK_BYTES)),
            )
        }
        Err(TrySendError::Full(mut block)) => {
            if *dropped_packets == 0 {
                eprintln!("mkra: BS2K capture queue full; discarding whole TS packets to keep the hardware drained");
            }
            *dropped_packets += (block.len() / 188) as u64;
            block.clear();
            Some(block)
        }
        Err(TrySendError::Disconnected(_)) => None,
    }
}

fn run(
    mut reader: XcReaderSource,
    sender: SyncSender<Vec<u8>>,
    free: Receiver<Vec<u8>>,
    stop: &AtomicBool,
    max_consumer_gap_us: &AtomicU64,
) -> io::Result<()> {
    let mut scratch = Vec::new();
    let mut block = Vec::with_capacity(BLOCK_BYTES);
    let mut captured_bytes = 0u64;
    let mut dropped_packets = 0u64;
    let mut short_blocks = 0u64;
    let mut empty_blocks = 0u64;
    let result = (|| {
        while !stop.load(Ordering::SeqCst) {
            scratch.clear();
            reader.read_into(&mut scratch)?;
            captured_bytes += scratch.len() as u64;
            if scratch.is_empty() {
                let complete = block.len() / 188 * 188;
                if complete > 0 {
                    let remainder = block.split_off(complete);
                    let Some(next) = enqueue(
                        &sender,
                        &free,
                        block,
                        &mut dropped_packets,
                        &mut short_blocks,
                    ) else {
                        return Ok(());
                    };
                    block = next;
                    block.extend_from_slice(&remainder);
                } else {
                    match sender.try_send(Vec::new()) {
                        Ok(()) => empty_blocks += 1,
                        Err(TrySendError::Disconnected(_)) => return Ok(()),
                        Err(TrySendError::Full(_)) => {}
                    }
                }
                continue;
            }
            let mut bytes = scratch.as_slice();
            while !bytes.is_empty() {
                let take = (BLOCK_BYTES - block.len()).min(bytes.len());
                block.extend_from_slice(&bytes[..take]);
                bytes = &bytes[take..];
                if block.len() == BLOCK_BYTES {
                    let Some(next) = enqueue(
                        &sender,
                        &free,
                        block,
                        &mut dropped_packets,
                        &mut short_blocks,
                    ) else {
                        return Ok(());
                    };
                    block = next;
                }
            }
        }
        Ok(())
    })();
    eprintln!(
        "mkra: BS2K capture closed: captured_packets={} dropped_packets={dropped_packets} short_blocks={short_blocks} empty_blocks={empty_blocks} consumer_gap_us={}",
        captured_bytes / 188,
        max_consumer_gap_us.load(Ordering::Relaxed),
    );
    result
}
