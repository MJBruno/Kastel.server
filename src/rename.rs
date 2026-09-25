use serde_json::{Value, json};

use crate::language::{BUILTINS, CONTEXTUAL_KEYWORDS, KEYWORDS, TYPE_NAMES};
use crate::lsp_position::offset_to_lsp;
use crate::text_util::{find_identifier_occurrences, find_word_at, is_identifier_char};
use crate::workspace::Workspace;

pub fn build_rename(
    workspace: &Workspace,
    uri: &str,
    line: u32,
    character: u32,
    new_name: &str,
) -> Option<Value> {
    if !is_valid_identifier(new_name) {
        return None;
    }

    let document = workspace.get(uri)?;

    let old_name = find_word_at(&document.text, line as usize, character as usize)?;
    if KEYWORDS.contains(&old_name)
        || BUILTINS.contains(&old_name)
        || TYPE_NAMES.contains(&old_name)
        || CONTEXTUAL_KEYWORDS.contains(&old_name)
    {
        return None;
    }

    /*
     * Le symbole peut être local au document courant
     * ou déclaré dans un module déjà chargé.
     */
    let symbol_exists = document.symbols.get(old_name).is_some()
        || workspace
            .iter()
            .any(|(_, document)| document.symbols.get(old_name).is_some());

    if !symbol_exists {
        return None;
    }

    /*
     * Le renommage retourne un WorkspaceEdit :
     *
     * {
     *   "changes": {
     *       "file:///...": [ edits ],
     *       "file:///...": [ edits ]
     *   }
     * }
     *
     * On parcourt tous les documents actuellement
     * présents dans le workspace.
     */
    let mut changes = serde_json::Map::new();

    for (document_uri, document) in workspace.iter() {
        let occurrences = find_identifier_occurrences(&document.text, old_name);

        if occurrences.is_empty() {
            continue;
        }

        let edits = occurrences
            .into_iter()
            .map(|offset| {
                let start = offset_to_lsp(&document.text, offset);

                let end = offset_to_lsp(&document.text, offset + old_name.len());

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
                    "newText": new_name
                })
            })
            .collect::<Vec<_>>();

        changes.insert(document_uri.clone(), Value::Array(edits));
    }

    if changes.is_empty() {
        return None;
    }

    Some(json!({
        "changes": changes
    }))
}

fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();

    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(is_identifier_char)
}
