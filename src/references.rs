use serde_json::{Value, json};

use crate::lsp_position::offset_to_lsp;
use crate::text_util::{find_identifier_occurrences, find_word_at};
use crate::workspace::Workspace;

pub fn build_references(
    workspace: &Workspace,
    uri: &str,
    line: u32,
    character: u32,
    include_declaration: bool,
) -> Option<Value> {
    let document = workspace.get(uri)?;

    let name = find_word_at(&document.text, line as usize, character as usize)?;

    /*
     * Le symbole doit exister dans le workspace.
     *
     * Il peut être déclaré :
     * - dans le fichier courant ;
     * - dans un module importé déjà chargé.
     */
    let symbol_exists = workspace
        .iter()
        .any(|(_, document)| document.symbols.get(name).is_some());

    if !symbol_exists {
        return None;
    }

    let mut locations = Vec::new();

    /*
     * Parcourt tous les documents actuellement
     * chargés dans le workspace.
     */
    for (document_uri, document) in workspace.iter() {
        for occurrence in find_identifier_occurrences(&document.text, name) {
            let (start_line, start_character) = offset_to_lsp(&document.text, occurrence);

            let end_offset = occurrence + name.len();

            let (end_line, end_character) = offset_to_lsp(&document.text, end_offset);

            /*
             * Lorsque la déclaration est exclue,
             * on l'identifie avec le span du symbole
             * du document concerné.
             */
            if !include_declaration && let Some(symbol) = document.symbols.get(name) {
                let (definition_line, definition_character) =
                    offset_to_lsp(&document.text, symbol.span.start);

                if start_line == definition_line && start_character == definition_character {
                    continue;
                }
            }

            locations.push(json!({
                "uri": document_uri,
                "range": {
                    "start": {
                        "line": start_line,
                        "character": start_character
                    },
                    "end": {
                        "line": end_line,
                        "character": end_character
                    }
                }
            }));
        }
    }

    Some(Value::Array(locations))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_all_identifier_occurrences() {
        let source = "const VALUE = 42\n\
             print(VALUE)\n\
             VALUE = 43\n";

        let occurrences = find_identifier_occurrences(source, "VALUE");

        assert_eq!(occurrences.len(), 3);
    }

    #[test]
    fn ignores_partial_identifier_matches() {
        let source = "VALUE\n\
             VALUE2\n\
             MY_VALUE\n\
             VALUE\n";

        let occurrences = find_identifier_occurrences(source, "VALUE");

        assert_eq!(occurrences.len(), 2);
    }

    #[test]
    fn find_word_returns_identifier_at_position() {
        let source = "print(VALUE)\n";

        let position = source.find("VALUE").unwrap();

        let result = find_word_at(source, 0, position);

        assert_eq!(result, Some("VALUE"));
    }

    #[test]
    fn find_word_handles_utf16_position() {
        // 😀 = 2 unités UTF-16 / 4 bytes.
        // character = 4 → au milieu de "VA|LUE".
        let source = "😀VALUE\n";

        let result = find_word_at(source, 0, 4);

        assert_eq!(result, Some("VALUE"));
    }
}
