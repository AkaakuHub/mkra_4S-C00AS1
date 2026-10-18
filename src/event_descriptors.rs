use std::collections::BTreeSet;

use crate::arib_text::encode_chunks;
use crate::si::Program;

fn text(value: &str) -> Result<Vec<u8>, String> {
    let mut chunks = encode_chunks(value, 250)
        .map_err(|character| format!("unsupported character U+{:04X}", character as u32))?;
    if chunks.len() > 1 {
        return Err("text exceeds descriptor length".into());
    }
    Ok(chunks.pop().unwrap_or_default())
}

fn extended_item(heading: &str, body: &str) -> Result<Vec<Vec<u8>>, String> {
    let heading = text(heading)?;
    if heading.is_empty() || heading.len() > 247 {
        return Err("extended item heading is empty or too long".into());
    }
    let mut chunks = encode_chunks(body, 247 - heading.len()).map_err(|character| {
        format!("cannot encode extended item at U+{:04X}", character as u32)
    })?;
    if chunks.is_empty() {
        chunks.push(Vec::new());
    }
    Ok(chunks
        .into_iter()
        .enumerate()
        .map(|(index, body)| {
            let heading = if index == 0 { heading.as_slice() } else { &[] };
            let mut payload = vec![
                0,
                b'j',
                b'p',
                b'n',
                (2 + heading.len() + body.len()) as u8,
                heading.len() as u8,
            ];
            payload.extend_from_slice(heading);
            payload.push(body.len() as u8);
            payload.extend_from_slice(&body);
            payload.push(0);
            payload
        })
        .collect())
}

pub fn descriptors(program: &Program, budget: usize, warnings: &mut BTreeSet<String>) -> Vec<u8> {
    let mut report = |field: &str, error: String| {
        warnings.insert(format!("event {} {}: {}", program.event_id, field, error));
    };
    let name = text(&program.name).unwrap_or_else(|error| {
        report("name", error);
        Vec::new()
    });
    let mut long_description = false;
    let description = match encode_chunks(&program.description, 250) {
        Ok(mut chunks)
            if chunks.len() <= 1 && name.len() + chunks.first().map_or(0, Vec::len) <= 250 =>
        {
            chunks.pop().unwrap_or_default()
        }
        Ok(_) => {
            long_description = true;
            Vec::new()
        }
        Err(character) => {
            report(
                "description",
                format!("unsupported character U+{:04X}", character as u32),
            );
            Vec::new()
        }
    };
    let mut output = vec![
        0x4d,
        (5 + name.len() + description.len()) as u8,
        b'j',
        b'p',
        b'n',
        name.len() as u8,
    ];
    output.extend_from_slice(&name);
    output.push(description.len() as u8);
    output.extend_from_slice(&description);
    if output.len() > budget {
        report("short event", "section length exceeded".into());
        return Vec::new();
    }
    if !program.genres.is_empty() {
        if program.genres.len() > 127
            || program.genres.iter().any(|genre| {
                [genre.lv1, genre.lv2, genre.un1, genre.un2]
                    .into_iter()
                    .any(|value| value > 15)
            })
        {
            report("genres", "invalid content descriptor".into());
        } else if output.len() + 2 + program.genres.len() * 2 > budget {
            report("genres", "section length exceeded".into());
        } else {
            output.extend_from_slice(&[0x54, (program.genres.len() * 2) as u8]);
            for genre in &program.genres {
                output.extend_from_slice(&[
                    (genre.lv1 << 4) | genre.lv2,
                    (genre.un1 << 4) | genre.un2,
                ]);
            }
        }
    }
    let mut extended = Vec::new();
    let mut extended_length = 0;
    let description = long_description.then_some(("番組概要", program.description.as_str()));
    for (heading, body) in description.into_iter().chain(
        program
            .extended
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    ) {
        let items = match extended_item(heading, body) {
            Ok(items) => items,
            Err(error) => {
                report(heading, error);
                continue;
            }
        };
        let length: usize = items.iter().map(|item| item.len() + 2).sum();
        if extended.len() + items.len() > 16 || output.len() + extended_length + length > budget {
            report(heading, "extended event or section length exceeded".into());
            continue;
        }
        extended_length += length;
        extended.extend(items);
    }

    let last = extended.len().saturating_sub(1) as u8;
    for (number, mut payload) in extended.into_iter().enumerate() {
        payload[0] = ((number as u8) << 4) | last;
        output.extend_from_slice(&[0x4e, payload.len() as u8]);
        output.extend_from_slice(&payload);
    }
    output
}
