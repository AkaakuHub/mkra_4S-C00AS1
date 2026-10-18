const ENCODING: &[u8] = include_bytes!("arib_text/encoding.bin");
const MARKS: [&str; 18] = [
    "[3D]",
    "[2ndScr]",
    "[2K]",
    "[4K]",
    "[8K]",
    "[5.1]",
    "[7.1]",
    "[22.2]",
    "[60P]",
    "[120P]",
    "[d]",
    "[HC]",
    "[HDR]",
    "[Hi-Res]",
    "[Lossless]",
    "[SHV]",
    "[UHD]",
    "[VOD]",
];

#[derive(Clone, Copy, PartialEq, Eq)]
struct Charset {
    selector: u8,
    double: bool,
}

fn mapped(character: char) -> Option<(Charset, u8, u8)> {
    if character == ' ' {
        return Some((
            Charset {
                selector: 0x4a,
                double: false,
            },
            0,
            0x20,
        ));
    }
    let code = character as u32;
    let mut low = 0;
    let mut high = ENCODING.len() / 8;
    while low < high {
        let middle = low + (high - low) / 2;
        let start = u32::from_be_bytes(ENCODING[middle * 8..middle * 8 + 4].try_into().ok()?);
        if start <= code {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let offset = low.checked_sub(1)? * 8;
    let start = u32::from_be_bytes(ENCODING[offset..offset + 4].try_into().ok()?);
    let entry = u32::from_be_bytes(ENCODING[offset + 4..offset + 8].try_into().ok()?);
    let delta = code - start;
    if delta >= entry & 0xff {
        return None;
    }
    let double = entry & 0x80000000 != 0;
    let row = ((entry >> 16) & 0x7f) as u8;
    let mut selector = ((entry >> 24) & 0x7f) as u8;
    if double && selector == 0x42 && row >= 0x75 {
        selector = 0x3b;
    }
    Some((
        Charset { selector, double },
        row,
        (((entry >> 8) & 0x7f) + delta) as u8,
    ))
}

fn append_character(
    character: char,
    output: &mut [u8; 64],
    length: &mut usize,
    current: &mut Option<Charset>,
) -> Result<(), char> {
    if character == '\n' {
        output[*length] = 0x0d;
        *length += 1;
        return Ok(());
    }
    let (charset, row, cell) = mapped(character).ok_or(character)?;
    if *current != Some(charset) {
        output[*length] = 0x1b;
        *length += 1;
        if charset.double {
            output[*length] = 0x24;
            *length += 1;
        }
        output[*length] = 0x28;
        output[*length + 1] = charset.selector;
        *length += 2;
        *current = Some(charset);
    }
    if charset.double {
        output[*length] = row;
        *length += 1;
    }
    output[*length] = cell;
    *length += 1;
    Ok(())
}

fn encode_token(
    original: char,
    mut current: Option<Charset>,
    output: &mut [u8; 64],
) -> Result<(usize, Option<Charset>), char> {
    let mut length = 0;
    if let Some(mark) = (original as u32)
        .checked_sub(0x1f19b)
        .and_then(|i| MARKS.get(i as usize))
    {
        for character in mark.chars() {
            append_character(character, output, &mut length, &mut current).map_err(|_| original)?;
        }
    } else {
        let normalized = match original {
            '\u{ff5e}' => '\u{301c}',
            '\u{ff0d}' => '-',
            other => other,
        };
        append_character(normalized, output, &mut length, &mut current).map_err(|_| original)?;
    }
    Ok((length, current))
}

pub fn encode_chunks(text: &str, limit: usize) -> Result<Vec<Vec<u8>>, char> {
    let mut chunks = Vec::new();
    let mut chunk = Vec::new();
    let mut current = None;
    let mut token = [0; 64];
    for original in text.chars() {
        let (mut length, mut next) = encode_token(original, current, &mut token)?;
        if !chunk.is_empty() && length > limit.saturating_sub(chunk.len()) {
            chunks.push(std::mem::take(&mut chunk));
            current = None;
            (length, next) = encode_token(original, current, &mut token)?;
        }
        let prefix = usize::from(chunk.is_empty());
        if length > limit.saturating_sub(prefix) || prefix > limit {
            return Err(original);
        }
        if chunk.is_empty() {
            chunk.push(0x0f);
        }
        chunk.extend_from_slice(&token[..length]);
        current = next;
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    Ok(chunks)
}
