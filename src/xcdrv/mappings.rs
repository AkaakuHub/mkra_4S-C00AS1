use std::{fs::File, io, ops::Range, os::unix::fs::MetadataExt};

pub(super) struct DeviceMappings(Vec<Range<u64>>);

impl DeviceMappings {
    pub(super) fn read(file: &File) -> io::Result<Self> {
        let metadata = file.metadata()?;
        let maps = std::fs::read_to_string("/proc/self/maps")?;
        Ok(Self::parse(&maps, metadata.dev(), metadata.ino()))
    }

    fn parse(maps: &str, device: u64, inode: u64) -> Self {
        // Linux dev_t encoding, as defined by bits/sysmacros.h.
        let major = ((device & 0xfff00) >> 8) | ((device & 0xfffff00000000000) >> 32);
        let minor = (device & 0xff) | ((device & 0xffffff00000) >> 12);
        Self(
            maps.lines()
                .filter_map(|line| {
                    let mut fields = line.split_ascii_whitespace();
                    let (start, end) = fields.next()?.split_once('-')?;
                    let permissions = fields.next()?.as_bytes();
                    fields.next()?;
                    let (map_major, map_minor) = fields.next()?.split_once(':')?;
                    let map_inode = fields.next()?.parse::<u64>().ok()?;
                    if permissions.len() != 4
                        || permissions[0] != b'r'
                        || permissions[3] != b's'
                        || inode == 0
                        || map_inode != inode
                        || u64::from_str_radix(map_major, 16).ok()? != major
                        || u64::from_str_radix(map_minor, 16).ok()? != minor
                    {
                        return None;
                    }
                    let start = u64::from_str_radix(start, 16).ok()?;
                    let end = u64::from_str_radix(end, 16).ok()?;
                    (start < end).then_some(start..end)
                })
                .collect(),
        )
    }

    pub(super) fn contains(&self, addr: u32, len: usize) -> bool {
        let start = u64::from(addr);
        start.checked_add(len as u64).is_some_and(|end| {
            self.0
                .iter()
                .any(|range| start >= range.start && end <= range.end)
        })
    }

    pub(super) fn overlaps(&self, addr: u32, len: u32) -> bool {
        // The driver rounds vm_munmap requests to page boundaries.
        let start = u64::from(addr) & !0xfff;
        let end = (u64::from(addr) + u64::from(len) + 0xfff) & !0xfff;
        self.0
            .iter()
            .any(|range| start < range.end && end > range.start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_readable_shared_mappings_of_the_open_file_are_kept() {
        let maps = concat!(
            "1000-2000 rw-s 1f331000 00:05 5176 /dev/vixs/xcodedrv\n",
            "2000-3000 r--s 40000000 00:05 5176 /device alias\n",
            "3000-4000 rw-p 00000000 00:05 5176 /dev/vixs/xcodedrv\n",
            "4000-5000 -w-s 00000000 00:05 5176 /dev/vixs/xcodedrv\n",
            "5000-6000 rw-s 00000000 00:06 5176 /dev/vixs/xcodedrv\n",
            "6000-7000 rw-s 00000000 00:05 5177 /dev/vixs/xcodedrv\n",
            "7000-8000 rw-p 00000000 00:00 0 [heap]\n",
            "8000-7000 rw-s 00000000 00:05 5176 /dev/vixs/xcodedrv\n",
            "malformed\n",
        );
        assert_eq!(
            DeviceMappings::parse(maps, 5, 5176).0,
            vec![0x1000..0x2000, 0x2000..0x3000]
        );
        assert!(DeviceMappings::parse(maps, 0, 0).0.is_empty());
    }

    #[test]
    fn linux_device_numbers_include_high_major_and_minor_bits() {
        let maps = "1000-2000 rw-s 00000000 12345:6789ab 5176 /driver\n";
        let device = (0x345u64 << 8) | (0x12000u64 << 32) | 0xab | (0x678900u64 << 12);
        assert_eq!(
            DeviceMappings::parse(maps, device, 5176).0,
            vec![0x1000..0x2000]
        );
    }

    #[test]
    fn containment_and_unmap_overlap_use_complete_ranges() {
        let mappings = DeviceMappings(vec![0x2000..0x4000]);
        assert!(mappings.contains(0x2000, 0x2000));
        assert!(!mappings.contains(0x1fff, 1));
        assert!(!mappings.contains(0x3fff, 2));
        assert!(!mappings.contains(u32::MAX, usize::MAX));
        assert!(mappings.overlaps(0x2fff, 1));
        assert!(mappings.overlaps(0x3fff, 2));
        assert!(!mappings.overlaps(0x1000, 0x1000));
        assert!(!mappings.overlaps(0x4000, 0x1000));
        assert!(mappings.overlaps(0x2001, 0));
    }
}
