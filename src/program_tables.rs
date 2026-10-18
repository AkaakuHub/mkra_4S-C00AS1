use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::remux4k::{AUDIO_PID, PCR_PID, PMT_PID, VIDEO_PID};
use crate::si::{crc32_mpeg, mirakurun_program_id, SiDb, TID_MH_EIT_PF};

pub fn convert_present_following(section: &[u8]) -> Option<Vec<u8>> {
    if section.len() < 18
        || section[0] != TID_MH_EIT_PF
        || section[5] & 1 == 0
        || section[6] > 1
        || section[7] > 1
        || crc32_mpeg(section) != 0
    {
        return None;
    }
    let end = section.len() - 4;
    let mut result = section[..14].to_vec();
    result[0] = 0x4e;
    result[13] = 0x4e;
    let mut offset = 14;
    while offset < end {
        if offset + 12 > end {
            return None;
        }
        let descriptor_length =
            (usize::from(section[offset + 10] & 15) << 8) | usize::from(section[offset + 11]);
        if offset + 12 + descriptor_length > end {
            return None;
        }
        result.extend_from_slice(&section[offset..offset + 10]);
        result.extend_from_slice(&[section[offset + 10] & 0xf0, 0]);
        offset += 12 + descriptor_length;
    }
    finish_section(&mut result);
    Some(result)
}

fn finish_section(section: &mut Vec<u8>) {
    let length = section.len() + 1;
    section[1] = (section[1] & 0xf0) | ((length >> 8) as u8 & 15);
    section[2] = length as u8;
    section.extend_from_slice(&crc32_mpeg(section).to_be_bytes());
}

fn packets(pid: u16, section: &[u8], continuity: &mut u8, output: &mut Vec<u8>) {
    let mut remaining = section;
    let mut first = true;
    while !remaining.is_empty() {
        let mut packet = [0xff; 188];
        packet[0] = 0x47;
        packet[1] = ((pid >> 8) as u8 & 31) | if first { 0x40 } else { 0 };
        packet[2] = pid as u8;
        packet[3] = 0x10 | *continuity;
        *continuity = (*continuity + 1) & 15;
        let start = if first {
            packet[4] = 0;
            5
        } else {
            4
        };
        let count = remaining.len().min(188 - start);
        packet[start..start + count].copy_from_slice(&remaining[..count]);
        output.extend_from_slice(&packet);
        remaining = &remaining[count..];
        first = false;
    }
}

pub struct ProgramTables {
    epg: Arc<RwLock<SiDb>>,
    network_id: u16,
    service_id: u16,
    audio_type: u8,
    next_update: Instant,
    sections: Vec<Vec<u8>>,
    pmt: Vec<u8>,
    tags: Option<(u8, u8)>,
    version: u8,
    continuity: u8,
}

impl ProgramTables {
    pub fn new(epg: Arc<RwLock<SiDb>>, network_id: u16, service_id: u16, audio_type: u8) -> Self {
        Self {
            epg,
            network_id,
            service_id,
            audio_type,
            next_update: Instant::now(),
            sections: Vec::new(),
            pmt: Vec::new(),
            tags: None,
            version: 0,
            continuity: 0,
        }
    }

    pub fn process(&mut self, output: &mut Vec<u8>, start: usize) {
        let update = Instant::now() >= self.next_update;
        if update {
            self.next_update = Instant::now() + Duration::from_millis(500);
            let Ok(db) = self.epg.read() else {
                eprintln!("mkra: cannot read EPG for program tables");
                return;
            };
            self.sections = db
                .present_following
                .iter()
                .filter(|((nid, sid, _), _)| *nid == self.network_id && *sid == self.service_id)
                .map(|(_, section)| section.clone())
                .collect();
            let tags = self
                .sections
                .iter()
                .find(|section| section[6] == 0 && section.len() >= 30)
                .and_then(|section| {
                    let eid = u16::from_be_bytes([section[14], section[15]]);
                    db.programs
                        .get(&mirakurun_program_id(self.network_id, self.service_id, eid))
                })
                .and_then(|program| {
                    Some((
                        u8::try_from(program.video.as_ref()?.component_tag).ok()?,
                        u8::try_from(
                            program
                                .audios
                                .iter()
                                .find(|audio| audio.is_main)?
                                .component_tag,
                        )
                        .ok()?,
                    ))
                });
            if let Some(tags) = tags.filter(|tags| Some(*tags) != self.tags) {
                self.tags = Some(tags);
                self.version = (self.version + 1) & 31;
                self.pmt = vec![
                    0x02,
                    0xb0,
                    0,
                    (self.service_id >> 8) as u8,
                    self.service_id as u8,
                    0xc1 | (self.version << 1),
                    0,
                    0,
                    0xe0 | (PCR_PID >> 8) as u8,
                    PCR_PID as u8,
                    0xf0,
                    0,
                ];
                for (kind, pid, tag) in [
                    (0x24, VIDEO_PID, tags.0),
                    (self.audio_type, AUDIO_PID, tags.1),
                ] {
                    self.pmt.extend_from_slice(&[
                        kind,
                        0xe0 | (pid >> 8) as u8,
                        pid as u8,
                        0xf0,
                        3,
                        0x52,
                        1,
                        tag,
                    ]);
                }
                finish_section(&mut self.pmt);
            }
        }
        if !self.pmt.is_empty() {
            for packet in output[start..].chunks_exact_mut(188) {
                let pid = (u16::from(packet[1] & 31) << 8) | u16::from(packet[2]);
                if pid == PMT_PID as u16 {
                    let version = ((packet[10] >> 1).wrapping_add(self.version)) & 31;
                    self.pmt[5] = 0xc1 | (version << 1);
                    self.pmt.truncate(self.pmt.len() - 4);
                    self.pmt.extend_from_slice(&crc32_mpeg(&self.pmt).to_be_bytes());
                    packet[1] = 0x40 | ((PMT_PID >> 8) as u8 & 31);
                    packet[3] = 0x10 | (packet[3] & 15);
                    packet[4] = 0;
                    packet[5..].fill(0xff);
                    packet[5..5 + self.pmt.len()].copy_from_slice(&self.pmt);
                }
            }
        }
        if update {
            for section in &self.sections {
                packets(18, section, &mut self.continuity, output);
            }
        }
    }
}
