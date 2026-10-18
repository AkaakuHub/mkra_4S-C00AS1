use crate::json;
use crate::si::{ScheduleSection, SiDb};

pub(super) fn render(sections: &[ScheduleSection]) -> String {
    json::array(|array| {
        for snapshot in sections {
            let section = &snapshot.data;
            let service_id = u16::from_be_bytes([section[3], section[4]]);
            let network_id = u16::from_be_bytes([section[10], section[11]]);
            let mut parsed = SiDb::new();
            parsed.ingest_section(section);
            array.obj(|object| {
                object
                    .num("receivedAt", snapshot.received_at_ms)
                    .num("serviceId", service_id)
                    .num("networkId", network_id)
                    .num(
                        "transportStreamId",
                        u16::from_be_bytes([section[8], section[9]]),
                    )
                    .num("tableId", section[0])
                    .num("versionNumber", (section[5] >> 1) & 31)
                    .num("sectionNumber", section[6])
                    .num("lastSectionNumber", section[7])
                    .num("segmentLastSectionNumber", section[12])
                    .arr("programs", |programs| {
                        for program in parsed.programs_of(network_id, service_id) {
                            programs.obj(|object| super::write_program(object, program));
                        }
                    });
            });
        }
    })
}
