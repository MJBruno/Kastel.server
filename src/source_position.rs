use crate::span::Span;

pub fn position_to_offset(source: &str, line: usize, column: usize) -> usize {
    let target_line = line.saturating_sub(1);
    let target_column = column.saturating_sub(1);

    let mut offset = 0;

    for (current_line, line_text) in source.split_inclusive('\n').enumerate() {
        if current_line == target_line {
            return offset + target_column.min(line_text.len());
        }

        offset += line_text.len();
    }

    source.len()
}

pub fn span_from_position(source: &str, line: usize, column: usize, length: usize) -> Span {
    let start = position_to_offset(source, line, column);

    let end = start.saturating_add(length).min(source.len());

    Span::new(start, end)
}
