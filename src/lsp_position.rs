use std::cmp::min;

pub fn offset_to_lsp(source: &str, offset: usize) -> (u32, u32) {
    let offset = min(offset, source.len());

    let prefix = &source[..offset];

    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();

    let line_start = prefix.rfind('\n').map(|index| index + 1).unwrap_or(0);

    let character = source[line_start..offset]
        .chars()
        .map(|c| c.len_utf16())
        .sum::<usize>();

    (line as u32, character as u32)
}
