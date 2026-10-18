use crate::tssrc::{TsSource, XcReaderSource};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub(super) const READ_TIMEOUT_MS: u32 = 200;
const BLOCK_BYTES: usize = 188 * 1024;
const QUEUE_BLOCKS: usize = 32;

pub(super) struct Capture {
    blocks: Receiver<Vec<u8>>,
    recycle: SyncSender<Vec<u8>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
}

impl Capture {
    pub(super) fn start(reader: XcReaderSource) -> io::Result<Self> {
        let (sender, blocks) = sync_channel(QUEUE_BLOCKS);
        let (recycle, free) = sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("bs2k-capture".into())
            .spawn(move || run(reader, sender, free, &worker_stop))?;
        Ok(Self {
            blocks,
            recycle,
            stop,
            worker: Some(worker),
        })
    }

    pub(super) fn recycle(&self, mut block: Vec<u8>) {
        if !block.is_empty() {
            block.clear();
            let _ = self.recycle.try_send(block);
        }
    }

    pub(super) fn read(&mut self) -> io::Result<Vec<u8>> {
        match self
            .blocks
            .recv_timeout(Duration::from_millis(u64::from(READ_TIMEOUT_MS)))
        {
            Ok(block) => Ok(block),
            Err(RecvTimeoutError::Timeout) => Ok(Vec::new()),
            Err(RecvTimeoutError::Disconnected) => {
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
) -> Option<Vec<u8>> {
    match sender.try_send(block) {
        Ok(()) => Some(
            free.try_recv()
                .unwrap_or_else(|_| Vec::with_capacity(BLOCK_BYTES)),
        ),
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
    mut reader: impl TsSource,
    sender: SyncSender<Vec<u8>>,
    free: Receiver<Vec<u8>>,
    stop: &AtomicBool,
) -> io::Result<()> {
    let mut scratch = Vec::new();
    let mut block = Vec::with_capacity(BLOCK_BYTES);
    let mut captured_bytes = 0u64;
    let mut dropped_packets = 0u64;
    let result = (|| {
        while !stop.load(Ordering::SeqCst) {
            scratch.clear();
            reader.read_into(&mut scratch)?;
            captured_bytes += scratch.len() as u64;
            let mut bytes = scratch.as_slice();
            while !bytes.is_empty() {
                let take = (BLOCK_BYTES - block.len()).min(bytes.len());
                block.extend_from_slice(&bytes[..take]);
                bytes = &bytes[take..];
                if block.len() == BLOCK_BYTES {
                    let Some(next) = enqueue(&sender, &free, block, &mut dropped_packets) else {
                        return Ok(());
                    };
                    block = next;
                }
            }
        }
        Ok(())
    })();
    eprintln!(
        "mkra: BS2K capture closed: captured_packets={} dropped_packets={dropped_packets}",
        captured_bytes / 188
    );
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct IntermittentSource(VecDeque<Vec<u8>>);

    impl TsSource for IntermittentSource {
        fn read_into(&mut self, out: &mut Vec<u8>) -> io::Result<usize> {
            let bytes = self
                .0
                .pop_front()
                .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
            out.extend_from_slice(&bytes);
            Ok(bytes.len())
        }

        fn describe(&self) -> String {
            "intermittent source".into()
        }
    }

    #[test]
    fn idle_reads_do_not_fragment_capture_queue() {
        let chunks: VecDeque<_> = (0..1024)
            .flat_map(|i| [vec![i as u8; 188], Vec::new()])
            .collect();
        let expected: Vec<_> = chunks.iter().flatten().copied().collect();
        let (sender, receiver) = sync_channel(QUEUE_BLOCKS);
        let (_recycle, free) = sync_channel(1);
        let result = run(
            IntermittentSource(chunks),
            sender,
            free,
            &AtomicBool::new(false),
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
        let blocks: Vec<_> = receiver.into_iter().collect();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks, vec![expected]);
    }
}
