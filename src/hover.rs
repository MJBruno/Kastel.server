use kastel::compiler::types::Type;
use serde_json::{Value, json};

use crate::class_index::MethodInfo;
use crate::completion::{
    class_source, detect_member_access, line_and_byte_to_offset, line_text_at,
    member_is_accessible,
};
use crate::language::{
    BUILTIN_FUNCTIONS, member_table,
};
use crate::lsp_position::offset_to_lsp;
use crate::symbols::SymbolKind;
use crate::text_util::{find_word_at, utf16_character_to_byte_index};
use crate::workspace::{Workspace, WorkspaceDocument};

pub fn build_hover(workspace: &Workspace, uri: &str, line: u32, character: u32) -> Option<Value> {
    let document = workspace.get(uri)?;
    let line_text = line_text_at(&document.text, line as usize);
    let byte_index = utf16_character_to_byte_index(line_text, character as usize);
    let word = find_word_at(&document.text, line as usize, character as usize)?;
    let word_start = find_word_start(line_text, byte_index);
    let range = word_range(document, line as usize, word_start, word_start + word.len());

    // Membre : `obj.method`, `obj.field`, `self.field`.
    if let Some(context) = detect_member_access(line_text, byte_index.max(word_start)) {
        if let Some(value) = build_member_hover(
            workspace,
            uri,
            document,
            &context.receiver,
            word,
            line as usize,
            word_start,
            &range,
        ) {
            return Some(value);
        }
        // `obj.membre` introuvable ou inaccessible (privé, non exporté) :
        // ne pas retomber sur un symbole global qui porterait le même nom.
        if !context.is_import_line {
            return None;
        }
    }

    // Builtins.
    if let Some((_name, signature, doc)) = BUILTIN_FUNCTIONS.iter().find(|(name, ..)| *name == word)
    {
        return Some(hover_value(
            format!("```kastel\n{}\n```\n\n{}", signature, doc),
            range,
        ));
    }

    // Fonctions globales et valeurs typées du document.
    if let Some(value) = build_local_symbol_hover(document, word, &range) {
        return Some(value);
    }

    // Symbole exporté/importé depuis un autre module.
    if let Some(value) = build_workspace_symbol_hover(workspace, uri, document, word, &range) {
        return Some(value);
    }

    None
}

fn build_local_symbol_hover(
    document: &WorkspaceDocument,
    word: &str,
    range: &Value,
) -> Option<Value> {
    let symbol = document.symbols.get(word)?;
    let mut contents = match symbol.kind {
        SymbolKind::Variable => {
            let ty = document
                .types
                .get(word)
                .map(ToString::to_string)
                .or_else(|| symbol.type_display.clone())
                .unwrap_or_else(|| "dynamic".to_string());
            format!("```kastel\nlet {}: {}\n```", word, ty)
        }
        SymbolKind::Function => {
            let signatures = document.types.function_signatures(word);
            if let Some(signatures) = signatures {
                if signatures.len() == 1 {
                    signatures[0].label()
                } else {
                    signatures
                        .iter()
                        .map(|s| s.label())
                        .collect::<Vec<_>>()
                        .join("\n\n")
                }
            } else {
                symbol
                    .signature
                    .clone()
                    .unwrap_or_else(|| format!("func {}(...)", word))
            }
        }
        SymbolKind::Class | SymbolKind::Interface | SymbolKind::Enum => {
            let kind = match symbol.kind {
                SymbolKind::Class => "class",
                SymbolKind::Interface => "interface",
                _ => "enum",
            };
            if let Some(info) = document.classes.get(word) {
                let bases = if info.bases.is_empty() {
                    String::new()
                } else {
                    format!(" : {}", info.bases.join(", "))
                };
                format!("```kastel\n{} {}{}\n```", kind, word, bases)
            } else {
                format!("```kastel\n{} {}\n```", kind, word)
            }
        }
        SymbolKind::Import => format!("```kastel\nmodule {}\n```", word),
        SymbolKind::TypeAlias => {
            format!(
                "```kastel\ntype {} = {}\n```",
                word,
                symbol
                    .type_display
                    .clone()
                    .unwrap_or_else(|| "dynamic".to_string())
            )
        }
    };

    if matches!(
        symbol.kind,
        SymbolKind::Class | SymbolKind::Interface | SymbolKind::Enum
    ) {
        if let Some(info) = document.classes.get(word) {
            if !info.fields.is_empty() {
                let label = if info.is_enum { "Variants" } else { "Fields" };
                contents.push_str(&format!(
                    "\n\n**{}**\n{}",
                    label,
                    info.fields
                        .iter()
                        .map(|name| format!("- `{}`", name))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
            if !info.methods.is_empty() {
                contents.push_str(&format!(
                    "\n\n**Methods**\n{}",
                    info.methods
                        .iter()
                        .map(MethodInfo::signature)
                        .map(|sig| format!("- `{}`", sig))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
        }
    }

    Some(hover_value(contents, range.clone()))
}

fn build_workspace_symbol_hover(
    workspace: &Workspace,
    current_uri: &str,
    document: &WorkspaceDocument,
    word: &str,
    range: &Value,
) -> Option<Value> {
    let current_file = crate::uri_util::uri_to_path(current_uri)?;
    let resolver = crate::module_resolver::ModuleResolver::new(
        workspace.root().map(std::path::Path::to_path_buf),
    );

    for import in parse_imports(&document.text) {
        let Some(module_path) = resolver.resolve(&current_file, &import.parts) else {
            continue;
        };
        let module_uri = crate::uri_util::path_to_uri(&module_path);
        let Some(module) = workspace.get(&module_uri) else {
            continue;
        };
        if let Some(symbol) = module.symbols.get(word).filter(|symbol| symbol.is_exported) {
            let ty = module
                .types
                .get(word)
                .map(ToString::to_string)
                .or_else(|| symbol.type_display.clone())
                .unwrap_or_else(|| "dynamic".to_string());
            return Some(hover_value(
                format!(
                    "```kastel\n{}\n```\n\nExported by `{}`",
                    symbol
                        .signature
                        .clone()
                        .unwrap_or_else(|| format!("{}: {}", word, ty)),
                    module_path.display()
                ),
                range.clone(),
            ));
        }
    }

    None
}

fn build_member_hover(
    workspace: &Workspace,
    uri: &str,
    document: &WorkspaceDocument,
    receiver: &str,
    member: &str,
    line: usize,
    word_start: usize,
    range: &Value,
) -> Option<Value> {
    let offset = line_and_byte_to_offset(&document.text, line, word_start);
    let receiver_types = infer_receiver_types(workspace, uri, document, receiver, offset);

    // `private`/`protected` : visibles uniquement via `self`.
    let allow_private = receiver == "self";

    for ty in receiver_types {
        if let Some(value) = hover_for_type_member(
            workspace,
            uri,
            document,
            &ty,
            member,
            allow_private,
            range,
        ) {
            return Some(value);
        }
    }

    None
}

fn infer_receiver_types(
    workspace: &Workspace,
    uri: &str,
    document: &WorkspaceDocument,
    receiver: &str,
    offset: usize,
) -> Vec<Type> {
    if receiver == "self" {
        return find_enclosing_class(document, offset)
            .map(|name| vec![Type::Named(name)])
            .unwrap_or_default();
    }
    // Pas de receveur `base`/`super` en Kastel : les classes n'héritent pas
    // d'implémentation, seulement d'interfaces (contrats purs).

    let mut current = receiver
        .split('.')
        .next()
        .and_then(|name| document.types.get(name).cloned())
        .map(|ty| vec![ty])
        .unwrap_or_default();

    let allow_private = receiver.split('.').next() == Some("self");

    for segment in receiver.split('.').skip(1) {
        let mut next = Vec::new();
        for ty in current {
            if let Some(module_types) = module_member_types(workspace, uri, document, &ty, segment)
            {
                next.extend(module_types);
            } else {
                next.extend(type_member_type(
                    workspace,
                    uri,
                    document,
                    &ty,
                    segment,
                    allow_private,
                ));
            }
        }
        current = next;
    }

    current
}

fn hover_for_type_member(
    workspace: &Workspace,
    current_uri: &str,
    document: &WorkspaceDocument,
    ty: &Type,
    member: &str,
    allow_private: bool,
    range: &Value,
) -> Option<Value> {
    match ty {
        Type::Union(members) => {
            let mut hovers = Vec::new();
            for member_ty in members {
                if let Some(hover) = hover_for_type_member(
                    workspace,
                    current_uri,
                    document,
                    member_ty,
                    member,
                    allow_private,
                    range,
                ) {
                    if let Some(text) = hover
                        .get("contents")
                        .and_then(|v| v.get("value"))
                        .and_then(Value::as_str)
                    {
                        hovers.push(text.to_string());
                    }
                }
            }
            if hovers.is_empty() {
                None
            } else {
                Some(hover_value(hovers.join("\n\n---\n\n"), range.clone()))
            }
        }
        Type::Named(class_name)
        | Type::Generic {
            name: class_name, ..
        } if class_source(workspace, current_uri, document, class_name).is_some() => {
            let (_, class_document) = class_source(workspace, current_uri, document, class_name)?;
            let classes = &class_document.classes;
            if !member_is_accessible(classes, class_name, member, allow_private) {
                return None;
            }
            if let Some(field) = classes.field(class_name, member) {
                if classes.is_enum(class_name) {
                    return Some(hover_value(
                        format!(
                            "```kastel\n{}.{}\n```\n\nVariant of enum `{}`",
                            class_name, member, class_name
                        ),
                        range.clone(),
                    ));
                }
                let ty = field
                    .type_annotation
                    .as_ref()
                    .map(crate::class_index::type_expr_display)
                    .unwrap_or_else(|| "dynamic".to_string());
                return Some(hover_value(
                    format!(
                        "```kastel\n{}{}: {}\n```\n\n{} of `{}`",
                        if field.is_static { "static " } else { "" },
                        member,
                        ty,
                        if field.is_static { "Static field" } else { "Field" },
                        class_name
                    ),
                    range.clone(),
                ));
            }
            if let Some(method) = classes.method(class_name, member) {
                let visibility = match method.visibility {
                    kastel::frontend::ast::Visibility::Private => "private",
                    kastel::frontend::ast::Visibility::Protected => "protected",
                    kastel::frontend::ast::Visibility::Public => "public",
                };
                return Some(hover_value(
                    format!(
                        "```kastel\n{}\n```\n\n{} method of `{}`",
                        method.signature(),
                        visibility,
                        class_name
                    ),
                    range.clone(),
                ));
            }
            None
        }
        Type::Record(fields) => fields
            .iter()
            .find(|(name, _)| name == member)
            .map(|(_, ty)| {
                hover_value(format!("```kastel\n{}: {}\n```", member, ty), range.clone())
            })
            .or_else(|| {
                member_table(ty).and_then(|table| table_hover(table, member, range))
            }),
        Type::Module(path) => {
            let current_file = crate::uri_util::uri_to_path(current_uri)?;
            let resolver = crate::module_resolver::ModuleResolver::new(
                workspace.root().map(std::path::Path::to_path_buf),
            );
            let parts = path
                .split('.')
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            let module_path = resolver.resolve(&current_file, &parts)?;
            let module_uri = crate::uri_util::path_to_uri(&module_path);
            let module = workspace.get(&module_uri)?;
            let symbol = module.symbols.get(member)?;
            if !symbol.is_exported && !crate::uri_util::same_uri(&module_uri, current_uri) {
                return None;
            }
            let text = symbol
                .signature
                .clone()
                .or_else(|| {
                    module
                        .types
                        .get(member)
                        .map(|ty| format!("{}: {}", member, ty))
                })
                .unwrap_or_else(|| format!("{}", member));
            Some(hover_value(
                format!(
                    "```kastel\n{}\n```\n\nExported by `{}`",
                    text,
                    module_path.display()
                ),
                range.clone(),
            ))
        }
        // Conteneurs, Option/Result, Task, Channel, Mutex, Semaphore,
        // WaitGroup, Iterator : tables de `language.rs`.
        other => member_table(other).and_then(|table| table_hover(table, member, range)),
    }
}

fn module_member_types(
    workspace: &Workspace,
    current_uri: &str,
    _document: &WorkspaceDocument,
    ty: &Type,
    segment: &str,
) -> Option<Vec<Type>> {
    let Type::Module(path) = ty else {
        return None;
    };
    let current_file = crate::uri_util::uri_to_path(current_uri)?;
    let resolver = crate::module_resolver::ModuleResolver::new(
        workspace.root().map(std::path::Path::to_path_buf),
    );
    let parts = path
        .split('.')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let module_path = resolver.resolve(&current_file, &parts)?;
    let module_uri = crate::uri_util::path_to_uri(&module_path);
    let module = workspace.get(&module_uri)?;
    // Un membre non exporté n'est pas accessible depuis l'extérieur.
    if !module
        .symbols
        .get(segment)
        .is_some_and(|symbol| symbol.is_exported)
    {
        return Some(Vec::new());
    }
    module.types.get(segment).cloned().map(|ty| vec![ty])
}

fn type_member_type(
    workspace: &Workspace,
    uri: &str,
    document: &WorkspaceDocument,
    ty: &Type,
    name: &str,
    allow_private: bool,
) -> Vec<Type> {
    match ty {
        Type::Union(members) => members
            .iter()
            .flat_map(|t| type_member_type(workspace, uri, document, t, name, allow_private))
            .collect(),
        Type::Named(class_name) => {
            let Some((_, class_document)) = class_source(workspace, uri, document, class_name)
            else {
                return Vec::new();
            };
            let classes = &class_document.classes;
            if !member_is_accessible(classes, class_name, name, allow_private) {
                return Vec::new();
            }
            if let Some(field) = classes.field(class_name, name) {
                vec![
                    field
                        .type_annotation
                        .as_ref()
                        .map(Type::from_type_expr)
                        .unwrap_or(Type::Dynamic),
                ]
            } else if let Some(method) = classes.method(class_name, name) {
                vec![Type::Function(method.function_type())]
            } else {
                Vec::new()
            }
        }
        _ => document
            .types
            .member_type(ty, name)
            .map(|t| vec![t])
            .unwrap_or_default(),
    }
}

fn table_hover(table: &[(&str, &str, &str)], member: &str, range: &Value) -> Option<Value> {
    table
        .iter()
        .find(|(name, ..)| *name == member)
        .map(|(_, signature, doc)| {
            hover_value(
                format!("```kastel\n{}\n```\n\n{}", signature, doc),
                range.clone(),
            )
        })
}

fn find_enclosing_class(document: &WorkspaceDocument, offset: usize) -> Option<String> {
    let masked = crate::text_util::mask_strings_and_comments(&document.text);
    document
        .classes
        .names()
        .filter_map(|name| {
            let needle = format!("class {}", name);
            let start = masked.find(&needle)?;
            let brace = masked[start..].find('{')? + start;
            let end = find_matching_brace(&document.text, brace)?;
            (start <= offset && offset <= end).then(|| (name.clone(), start))
        })
        .max_by_key(|(_, start)| *start)
        .map(|(name, _)| name)
}

fn find_matching_brace(source: &str, open: usize) -> Option<usize> {
    let masked = crate::text_util::mask_strings_and_comments(source);
    let mut depth = 0usize;
    for (index, byte) in masked.bytes().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_word_start(line: &str, byte_index: usize) -> usize {
    let bytes = line.as_bytes();
    let mut start = byte_index.min(bytes.len());
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    start
}

fn word_range(document: &WorkspaceDocument, line: usize, start: usize, end: usize) -> Value {
    let start_offset = line_and_byte_to_offset(&document.text, line, start);
    let end_offset = line_and_byte_to_offset(&document.text, line, end);
    let s = offset_to_lsp(&document.text, start_offset);
    let e = offset_to_lsp(&document.text, end_offset);
    json!({"start":{"line":s.0,"character":s.1},"end":{"line":e.0,"character":e.1}})
}

fn hover_value(contents: String, range: Value) -> Value {
    json!({
        "contents": {"kind":"markdown","value":contents},
        "range": range,
    })
}

#[derive(Debug)]
struct ImportPath {
    parts: Vec<String>,
}

fn parse_imports(source: &str) -> Vec<ImportPath> {
    let mut result = Vec::new();
    for line in source.lines().map(str::trim) {
        let line = line.trim_end_matches(';');
        if let Some(rest) = line.strip_prefix("import ") {
            if rest.starts_with('{') || rest.starts_with('*') {
                continue;
            }
            let parts = rest
                .split('.')
                .map(str::to_string)
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>();
            if !parts.is_empty() {
                result.push(ImportPath { parts });
            }
        } else if let Some(rest) = line.strip_prefix("from ") {
            if let Some((module, _)) = rest.split_once(" import ") {
                let parts = module
                    .split('.')
                    .map(str::to_string)
                    .filter(|p| !p.is_empty())
                    .collect::<Vec<_>>();
                if !parts.is_empty() {
                    result.push(ImportPath { parts });
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::LIST_METHODS;
    use crate::workspace::Workspace;

    #[test]
    fn hover_builtin_contains_signature() {
        let mut ws = Workspace::new();
        ws.open("file:///main.ks".to_string(), 1, "println(42)".to_string());
        let hover = build_hover(&ws, "file:///main.ks", 0, 2).unwrap();
        assert!(
            hover["contents"]["value"]
                .as_str()
                .unwrap()
                .contains("println(value)")
        );
    }

    #[test]
    fn hover_does_not_advertise_removed_list_api() {
        assert!(
            LIST_METHODS
                .iter()
                .all(|(name, ..)| *name != "push" && *name != "length")
        );
    }

    const ACCESS_SOURCE: &str = "class Persone {
    private let nom: str;

    func get_nom() -> str {
        return self.nom;
    }
}

let p = new Persone(\"G\");
let a = p.nom;
let b = p.get_nom();
";

    fn access_workspace() -> Workspace {
        let mut workspace = Workspace::new();
        workspace.open(
            "file:///main.ks".to_string(),
            1,
            ACCESS_SOURCE.to_string(),
        );
        workspace
    }

    #[test]
    fn hover_hides_private_member_outside_the_class() {
        let workspace = access_workspace();
        // `p.nom` : champ privé, pas de survol depuis l'extérieur.
        assert!(build_hover(&workspace, "file:///main.ks", 9, 11).is_none());
    }

    #[test]
    fn hover_shows_public_member_outside_the_class() {
        let workspace = access_workspace();
        let hover = build_hover(&workspace, "file:///main.ks", 10, 12).expect("survol attendu");
        assert!(hover.to_string().contains("get_nom"));
    }

    #[test]
    fn hover_shows_private_member_through_self() {
        let workspace = access_workspace();
        let hover = build_hover(&workspace, "file:///main.ks", 4, 21).expect("survol attendu");
        assert!(hover.to_string().contains("nom"));
    }
}
