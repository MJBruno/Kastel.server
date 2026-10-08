use serde_json::{Value, json};

use crate::analyzer::Diagnostic;
use crate::position::kastel_to_lsp;

pub fn build_diagnostics(uri: &str, source: &str, diagnostics: Vec<Diagnostic>) -> Value {
    let diagnostics = diagnostics
        .into_iter()
        .map(|diagnostic| {
            let (line, character) = kastel_to_lsp(source, diagnostic.line, diagnostic.column);

            let end_character =
                diagnostic_end_character(source, diagnostic.line, diagnostic.column, character);

            json!({
                "range": {
                    "start": {
                        "line": line,
                        "character": character
                    },
                    "end": {
                        "line": line,
                        "character": end_character
                    }
                },
                "severity": diagnostic.severity,
                "source": "kastel",
                "code": diagnostic.code,
                "message": diagnostic.message
            })
        })
        .collect::<Vec<_>>();

    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": diagnostics
        }
    })
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Calcule la position UTF-16 de fin du diagnostic.
///
/// `column` est un index de **caractère** 1-based (même convention
/// que `LexerError::column`, `ParserError::column`, et que
/// `crate::position::kastel_to_lsp`).
///
/// Si le diagnostic démarre sur un mot (identifiant, mot-clé, nombre), le
/// soulignement couvre le mot entier ; sinon un seul caractère.
fn diagnostic_end_character(source: &str, line: usize, column: usize, start_character: u32) -> u32 {
    let line_index = line.saturating_sub(1);

    let Some(line_text) = source.lines().nth(line_index) else {
        return start_character.saturating_add(1);
    };

    let column_index = column.saturating_sub(1);

    let mut characters = line_text.chars().skip(column_index);

    let Some(first) = characters.next() else {
        return start_character.saturating_add(1);
    };

    let mut length = first.len_utf16() as u32;

    if is_word_character(first) {
        for character in characters {
            if !is_word_character(character) {
                break;
            }

            length += character.len_utf16() as u32;
        }
    }

    start_character.saturating_add(length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_diagnostic_spans_whole_word() {
        let source = "const VALUE = 42\n";

        let end = diagnostic_end_character(source, 1, 7, 6);

        assert_eq!(end, 11);
    }

    #[test]
    fn diagnostic_on_symbol_spans_one_character() {
        let source = "let x = a + b;\n";

        let end = diagnostic_end_character(source, 1, 11, 10);

        assert_eq!(end, 11);
    }

    #[test]
    fn diagnostic_stops_at_end_of_word() {
        let source = "foo(Add, Ord)\n";

        // `Add` commence à la colonne 5 (index 4) et se termine avant la virgule.
        assert_eq!(diagnostic_end_character(source, 1, 5, 4), 7);
    }

    #[test]
    fn utf16_diagnostic_spans_surrogate_character() {
        let source = "😀abc\n";

        let end = diagnostic_end_character(source, 1, 1, 0);

        assert_eq!(end, 2);
    }

    #[test]
    fn utf16_diagnostic_does_not_panic_on_mid_character_column() {
        let source = "😀abc\n";

        let end = diagnostic_end_character(source, 1, 2, 2);

        assert_eq!(end, 5);
    }

    #[test]
    fn invalid_position_keeps_valid_one_character_range() {
        let source = "abc\n";

        let end = diagnostic_end_character(source, 10, 10, 5);

        assert_eq!(end, 6);
    }

    #[test]
    fn column_past_end_of_line_keeps_valid_range() {
        let source = "abc\n";

        let end = diagnostic_end_character(source, 1, 100, 0);

        assert_eq!(end, 1);
    }

    #[test]
    fn build_diagnostics_produces_utf16_range_on_emoji_line() {
        let source = "😀abc\n";

        let diagnostic = Diagnostic::error("test".to_string(), 1, 2, "test");

        let value = build_diagnostics("file:///test.ks", source, vec![diagnostic]);

        let diag = &value["params"]["diagnostics"][0];

        assert_eq!(diag["range"]["start"]["character"], 2);

        assert_eq!(diag["range"]["end"]["character"], 5);

        assert_eq!(diag["severity"], 1);
        assert_eq!(diag["code"], "test");
    }
}
