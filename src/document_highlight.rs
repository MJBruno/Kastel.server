use serde_json::{Value, json};

use crate::lsp_position::offset_to_lsp;
use crate::text_util::{find_identifier_occurrences, find_word_at};
use crate::workspace::Workspace;

/// Retourne un `DocumentHighlight[]` : toutes les occurrences de
/// l'identifiant sous le curseur, dans le document courant.
///
/// `kind` : 1 = Text, 2 = Read, 3 = Write (LSP).
/// On utilise 1 (Text) pour rester simple — VSCode n'utilise pas
/// fortement cette distinction à l'affichage.
pub fn build_document_highlight(
    workspace: &Workspace,
    uri: &str,
    line: u32,
    character: u32,
) -> Option<Value> {
    let document = workspace.get(uri)?;

    let name = find_word_at(&document.text, line as usize, character as usize)?;

    /*
     * On surligne toutes les occurrences trouvées, qu'elles
     * correspondent à une déclaration indexée ou à une simple
     * référence. Cela reproduit le comportement de VSCode
     * (y compris pour les variables locales non indexées).
     */
    let occurrences = find_identifier_occurrences(&document.text, name);

    if occurrences.is_empty() {
        return None;
    }

    let highlights = occurrences
        .into_iter()
        .map(|offset| {
            let start = offset_to_lsp(&document.text, offset);

            let end = offset_to_lsp(&document.text, offset + name.len());

            json!({
                "range": {
                    "start": {
                        "line": start.0,
                        "character": start.1
                    },
                    "end": {
                        "line": end.0,
                        "character": end.1
                    }
                },
                "kind": 1
            })
        })
        .collect::<Vec<_>>();

    Some(Value::Array(highlights))
}

#[cfg(test)]
mod tests {
    use crate::workspace::Workspace;

    use super::*;

    fn workspace_with_main(text: &str) -> Workspace {
        let mut workspace = Workspace::new();

        workspace.open("file:///main.ks".to_string(), 1, text.to_string());

        workspace
    }

    #[test]
    fn highlights_all_occurrences_of_identifier() {
        let source = "const VALUE = 42\n\
             print(VALUE)\n\
             VALUE = 43\n";

        let workspace = workspace_with_main(source);

        // Position sur VALUE de la ligne 0.
        let result = build_document_highlight(&workspace, "file:///main.ks", 0, 6)
            .expect("expected highlights");

        let array = result.as_array().expect("expected array");

        assert_eq!(array.len(), 3);
    }

    #[test]
    fn returns_none_when_cursor_off_identifier() {
        let source = "a + b\n";

        let workspace = workspace_with_main(source);

        let result = build_document_highlight(&workspace, "file:///main.ks", 0, 2);

        assert!(result.is_none());
    }

    #[test]
    fn highlights_handle_utf16_positions() {
        let source = "😀VALUE\n\
             VALUE\n";

        let workspace = workspace_with_main(source);

        // 2 unités UTF-16 pour 😀 + 2 pour "VA" → character = 4
        let result = build_document_highlight(&workspace, "file:///main.ks", 0, 4)
            .expect("expected highlights");

        let array = result.as_array().unwrap();

        assert_eq!(array.len(), 2);

        // Vérifie que la 1ère occurrence commence après l'emoji
        // en unités UTF-16 (donc à character = 2).
        assert_eq!(array[0]["range"]["start"]["character"], 2);
    }

    #[test]
    fn returns_none_when_uri_unknown() {
        let workspace = Workspace::new();

        let result = build_document_highlight(&workspace, "file:///missing.ks", 0, 0);

        assert!(result.is_none());
    }
}
