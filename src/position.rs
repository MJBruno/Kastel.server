pub fn kastel_to_lsp(source: &str, line: usize, column: usize) -> (u32, u32) {
    let line_index = line.saturating_sub(1);

    let source_line = source.lines().nth(line_index).unwrap_or("");

    let column_index = column.saturating_sub(1);

    let character = source_line
        .chars()
        .take(column_index)
        .map(|c| c.len_utf16())
        .sum::<usize>();

    (line_index as u32, character as u32)
}
