use std::path::PathBuf;

use kastel::compiler::types::Type;
use kastel::frontend::ast::Statement;
use serde_json::{Value, json};

use crate::completion::{
    class_source, detect_member_access, infer_receiver_types, line_and_byte_to_offset,
    member_is_accessible,
};
use crate::lsp_position::offset_to_lsp;
use crate::module_resolver::ModuleResolver;
use crate::text_util::{find_word_at, utf16_character_to_byte_index};
use crate::uri_util::{path_to_uri, same_uri, uri_to_path};
use crate::workspace::{Workspace, WorkspaceDocument};

fn location(uri: &str, document: &WorkspaceDocument, start: usize, end: usize) -> Value {
    let (start_line, start_character) = offset_to_lsp(&document.text, start);
    let (end_line, end_character) = offset_to_lsp(&document.text, end);

    json!({
        "uri": uri,
        "range": {
            "start": { "line": start_line, "character": start_character },
            "end": { "line": end_line, "character": end_character }
        }
    })
}

fn symbol_location(uri: &str, document: &WorkspaceDocument, name: &str) -> Option<Value> {
    let symbol = document.symbols.get(name)?;
    Some(location(uri, document, symbol.span.start, symbol.span.end))
}

pub fn build_definition(
    workspace: &Workspace,
    uri: &str,
    line: u32,
    character: u32,
) -> Option<Value> {
    let document = workspace.get(uri)?;
    let line_index = line as usize;
    let char_index = character as usize;
    let line_text = document.text.split('\n').nth(line_index)?;
    let byte_index = utf16_character_to_byte_index(line_text, char_index);
    let word = find_word_at(&document.text, line_index, char_index)?;
    let absolute = line_and_byte_to_offset(&document.text, line_index, byte_index);

    // 1. Accès membre : `obj.method`, `obj.field`, `self.method`,
    //    `module.export`.
    if let Some(context) = detect_member_access(line_text, byte_index) {
        if let Some(result) =
            resolve_member_definition(workspace, uri, document, &context.receiver, word, absolute)
        {
            return Some(result);
        }
        // `obj.membre` privé, non exporté ou introuvable : ne pas retomber sur
        // un symbole global qui porterait le même nom.
        if !context.is_import_line {
            return None;
        }
    }

    // 2. Symbole local/courant : fonctions, classes, interfaces, types,
    //    variables et imports.
    if let Some(result) = symbol_location(uri, document, word) {
        return Some(result);
    }

    // 3. Symbole issu d'un import Kastel (`import`, `from ... import ...`).
    resolve_imported_definition(workspace, uri, document, word)
}

fn resolve_member_definition(
    workspace: &Workspace,
    uri: &str,
    document: &WorkspaceDocument,
    receiver: &str,
    member: &str,
    offset: usize,
) -> Option<Value> {
    // `private`/`protected` : accessibles uniquement via `self`.
    let allow_private = receiver == "self";
    let receiver_types = infer_receiver_types(workspace, uri, document, receiver, offset);

    for ty in receiver_types {
        if let Some(result) =
            resolve_single_member_definition(workspace, uri, document, &ty, member, allow_private)
        {
            return Some(result);
        }
    }

    if receiver == "self" {
        if let Some(class_name) = find_enclosing_class(document, offset) {
            return class_member_definition(workspace, uri, document, &class_name, member, true);
        }
    }

    None
}

fn resolve_single_member_definition(
    workspace: &Workspace,
    uri: &str,
    document: &WorkspaceDocument,
    ty: &Type,
    member: &str,
    allow_private: bool,
) -> Option<Value> {
    match ty {
        Type::Named(class_name)
        | Type::Generic {
            name: class_name, ..
        } => class_member_definition(workspace, uri, document, class_name, member, allow_private),
        Type::Module(path) => module_member_definition(workspace, uri, path, member),
        Type::Union(members) => members.iter().find_map(|member_ty| {
            resolve_single_member_definition(
                workspace,
                uri,
                document,
                member_ty,
                member,
                allow_private,
            )
        }),
        _ => None,
    }
}

/// Définition d'un membre de classe. La classe peut venir d'un module importé
/// (exportée uniquement) : la position retournée pointe alors dans ce module.
/// Un membre `private`/`protected` n'est résolu que depuis `self`.
fn class_member_definition(
    workspace: &Workspace,
    uri: &str,
    document: &WorkspaceDocument,
    class_name: &str,
    member: &str,
    allow_private: bool,
) -> Option<Value> {
    let (class_uri, class_document) = class_source(workspace, uri, document, class_name)?;
    let classes = &class_document.classes;

    if !member_is_accessible(classes, class_name, member, allow_private) {
        return None;
    }

    let owner = if classes.method(class_name, member).is_some() {
        classes.method_owner(class_name, member)
    } else if classes.field(class_name, member).is_some() {
        find_field_owner(classes, class_name, member)
    } else {
        None
    }?;

    let span = find_member_declaration_span(&class_document.text, &owner, member)?;
    Some(location(&class_uri, class_document, span.0, span.1))
}

fn find_field_owner(
    classes: &crate::class_index::ClassIndex,
    class_name: &str,
    field_name: &str,
) -> Option<String> {
    let mut visited = std::collections::HashSet::new();
    find_field_owner_inner(classes, class_name, field_name, &mut visited)
}

fn find_field_owner_inner(
    classes: &crate::class_index::ClassIndex,
    class_name: &str,
    field_name: &str,
    visited: &mut std::collections::HashSet<String>,
) -> Option<String> {
    if !visited.insert(class_name.to_string()) {
        return None;
    }

    let info = classes.get(class_name)?;
    if info.fields.iter().any(|field| field == field_name) {
        return Some(class_name.to_string());
    }

    for base in &info.bases {
        if let Some(owner) = find_field_owner_inner(classes, base, field_name, visited) {
            return Some(owner);
        }
    }

    None
}

fn find_member_declaration_span(
    source: &str,
    class_name: &str,
    member: &str,
) -> Option<(usize, usize)> {
    let class_span = find_type_span(source, class_name)?;
    let masked = crate::text_util::mask_strings_and_comments(source);
    let region = &masked[class_span.0..=class_span.1];
    let needle = format!("func {}", member);
    if let Some(relative) = region.find(&needle) {
        let start = class_span.0 + relative + 5;
        if is_identifier_at(&masked, start, member) {
            return Some((start, start + member.len()));
        }
    }

    // Les champs sont écrits `let field...` ou `private let field...` /
    // `public let field...`.
    for prefix in ["let ", "const ", "private let ", "public let "] {
        let needle = format!("{}{}", prefix, member);
        if let Some(relative) = region.find(&needle) {
            let start = class_span.0 + relative + prefix.len();
            if is_identifier_at(&masked, start, member) {
                return Some((start, start + member.len()));
            }
        }
    }

    // `enum Color { Red, Green, Blue }` : un variant n'a ni `func` ni `let`
    // devant lui, juste son nom nu dans le corps de l'enum.
    let mut search_from = 0;
    while let Some(relative) = region[search_from..].find(member) {
        let start = class_span.0 + search_from + relative;
        if is_identifier_at(&masked, start, member) {
            return Some((start, start + member.len()));
        }
        search_from += relative + 1;
    }

    None
}

fn is_identifier_at(source: &str, start: usize, name: &str) -> bool {
    if start + name.len() > source.len() || &source[start..start + name.len()] != name {
        return false;
    }
    crate::text_util::is_identifier_boundary(source, start, start + name.len())
}

fn module_member_definition(
    workspace: &Workspace,
    current_uri: &str,
    module_path: &str,
    member: &str,
) -> Option<Value> {
    let current_file = uri_to_path(current_uri)?;
    let resolver = ModuleResolver::new(workspace.root().map(PathBuf::from));
    let parts = module_path
        .split('.')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let module_path = resolver.resolve(&current_file, &parts)?;
    let module_uri = path_to_uri(&module_path);
    let module = workspace.get(&module_uri)?;
    if !module
        .symbols
        .get(member)
        .map(|symbol| symbol.is_exported || same_uri(&module_uri, current_uri))
        .unwrap_or(false)
    {
        return None;
    }
    symbol_location(&module_uri, module, member)
}

fn resolve_imported_definition(
    workspace: &Workspace,
    current_uri: &str,
    document: &WorkspaceDocument,
    name: &str,
) -> Option<Value> {
    let current_file = uri_to_path(current_uri)?;
    let resolver = ModuleResolver::new(workspace.root().map(PathBuf::from));

    for statement in &document.statements {
        if let Some(result) = resolve_import_statement(
            workspace,
            current_uri,
            &current_file,
            statement,
            name,
            &resolver,
        ) {
            return Some(result);
        }
    }

    None
}

fn resolve_import_statement(
    workspace: &Workspace,
    current_uri: &str,
    current_file: &std::path::Path,
    statement: &Statement,
    name: &str,
    resolver: &ModuleResolver,
) -> Option<Value> {
    match statement {
        Statement::Positioned { statement, .. } | Statement::Export { statement } => {
            resolve_import_statement(
                workspace,
                current_uri,
                current_file,
                statement,
                name,
                resolver,
            )
        }
        Statement::Import { path } => {
            let imported_name = path.last()?;
            if imported_name != name {
                return None;
            }
            let resolution = resolver.resolve_import(current_file, path).ok()?;
            resolve_import_resolution(workspace, current_uri, resolution, name)
        }
        Statement::FromImport { module, items } => {
            for item in items {
                let binding = item.alias.as_ref().unwrap_or(&item.name);
                if binding == name {
                    let mut parts = module.parts.clone();
                    parts.push(item.name.clone());
                    if let Ok(resolution) = resolver.resolve_import(current_file, &parts) {
                        if let Some(result) = resolve_import_resolution(
                            workspace,
                            current_uri,
                            resolution,
                            &item.name,
                        ) {
                            return Some(result);
                        }
                    }
                }
            }
            None
        }
        Statement::Block(body)
        | Statement::While { body, .. }
        | Statement::ForIn { body, .. }
        | Statement::Try { try_body: body, .. } => body.iter().find_map(|child| {
            resolve_import_statement(workspace, current_uri, current_file, child, name, resolver)
        }),
        Statement::If {
            then_branch,
            else_branch,
            ..
        } => then_branch
            .iter()
            .find_map(|child| {
                resolve_import_statement(
                    workspace,
                    current_uri,
                    current_file,
                    child,
                    name,
                    resolver,
                )
            })
            .or_else(|| {
                else_branch.as_ref().and_then(|branch| {
                    branch.iter().find_map(|child| {
                        resolve_import_statement(
                            workspace,
                            current_uri,
                            current_file,
                            child,
                            name,
                            resolver,
                        )
                    })
                })
            }),
        Statement::Match { arms, .. } => {
            arms.iter()
                .flat_map(|arm| arm.body.iter())
                .find_map(|child| {
                    resolve_import_statement(
                        workspace,
                        current_uri,
                        current_file,
                        child,
                        name,
                        resolver,
                    )
                })
        }
        _ => None,
    }
}

fn resolve_import_resolution(
    workspace: &Workspace,
    current_uri: &str,
    resolution: kastel::module::resolver::ImportResolution,
    name: &str,
) -> Option<Value> {
    use kastel::module::resolver::ImportResolution;

    match resolution {
        ImportResolution::Module(path) => {
            let module_uri = path_to_uri(&path);
            let module = workspace.get(&module_uri)?;
            let symbol = module.symbols.get(name)?;
            if !symbol.is_exported && !same_uri(&module_uri, current_uri) {
                return None;
            }
            symbol_location(&module_uri, module, name)
        }
        ImportResolution::Export {
            module,
            name: export_name,
        } => {
            let module_uri = path_to_uri(&module);
            let module_doc = workspace.get(&module_uri)?;
            let symbol = module_doc.symbols.get(&export_name)?;
            if !symbol.is_exported && !same_uri(&module_uri, current_uri) {
                return None;
            }
            symbol_location(&module_uri, module_doc, &export_name)
        }
    }
}

fn find_enclosing_class(document: &WorkspaceDocument, offset: usize) -> Option<String> {
    document
        .classes
        .names()
        .filter_map(|name| {
            let span = find_type_span(&document.text, name)?;
            (span.0 <= offset && offset <= span.1).then(|| (name.clone(), span.0))
        })
        .max_by_key(|(_, start)| *start)
        .map(|(name, _)| name)
}

fn find_type_span(source: &str, name: &str) -> Option<(usize, usize)> {
    let masked = crate::text_util::mask_strings_and_comments(source);
    let mut best = None;

    // `enum` est inclus : ses méthodes acceptent `self` (elles sont
    // vérifiées via `check_class`, comme les classes — voir
    // `type_checker::Statement::Enum`), et ses variants sont résolus comme
    // des membres qualifiés (`Color.Red`).
    for kind in ["class", "interface", "enum"] {
        let needle = format!("{} {}", kind, name);
        if let Some(start) = masked.find(&needle) {
            let brace = masked[start..].find('{')? + start;
            let end = find_matching_brace(&masked, brace)?;
            if best.map(|(previous, _)| start < previous).unwrap_or(true) {
                best = Some((start, end));
            }
        }
    }

    best
}

fn find_matching_brace(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    for index in open..bytes.len() {
        match bytes[index] {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_member_declaration() {
        let source = "class Person {\n    private let name: str = \"Bruno\";\n    func greet() -> str {\n        return name;\n    }\n}\n";
        let span = find_member_declaration_span(source, "Person", "greet").unwrap();
        assert_eq!(&source[span.0..span.1], "greet");
    }

    #[test]
    fn finds_private_field_declaration() {
        let source = "class Person {\n    private let name: str = \"Bruno\";\n}\n";
        let span = find_member_declaration_span(source, "Person", "name").unwrap();
        assert_eq!(&source[span.0..span.1], "name");
    }

    #[test]
    fn finds_interface_method_declaration() {
        let source = "interface Named {\n    func name() -> str;\n}\n";
        let span = find_member_declaration_span(source, "Named", "name").unwrap();
        assert_eq!(&source[span.0..span.1], "name");
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
    fn definition_hides_private_member_outside_the_class() {
        let workspace = access_workspace();
        assert!(build_definition(&workspace, "file:///main.ks", 9, 11).is_none());
    }

    #[test]
    fn definition_finds_public_member_outside_the_class() {
        let workspace = access_workspace();
        let result = build_definition(&workspace, "file:///main.ks", 10, 12)
            .expect("définition attendue");
        // Déclaration `func get_nom` : ligne 3 (0-based).
        assert_eq!(result["range"]["start"]["line"], 3);
    }

    #[test]
    fn definition_finds_private_member_through_self() {
        let workspace = access_workspace();
        let result = build_definition(&workspace, "file:///main.ks", 4, 21)
            .expect("définition attendue");
        // Déclaration `private let nom` : ligne 1 (0-based).
        assert_eq!(result["range"]["start"]["line"], 1);
    }
}
