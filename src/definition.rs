use std::path::PathBuf;

use kastel::compiler::types::Type;
use kastel::frontend::ast::Statement;
use serde_json::{Value, json};

use crate::completion::{detect_member_access, infer_receiver_types, line_and_byte_to_offset};
use crate::lsp_position::offset_to_lsp;
use crate::module_resolver::ModuleResolver;
use crate::text_util::{find_word_at, utf16_character_to_byte_index};
use crate::uri_util::{path_to_uri, uri_to_path};
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

    // 1. Accès membre : `obj.method`, `obj.field`, `this.method`,
    //    `module.export`.
    if let Some(context) = detect_member_access(line_text, byte_index) {
        if let Some(result) =
            resolve_member_definition(workspace, uri, document, &context.receiver, word, absolute)
        {
            return Some(result);
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
    let receiver_types = infer_receiver_types(workspace, uri, document, receiver, offset);

    for ty in receiver_types {
        match ty {
            Type::Named(class_name) => {
                if let Some(result) = class_member_definition(document, uri, &class_name, member) {
                    return Some(result);
                }
            }
            Type::Module(path) => {
                if let Some(result) = module_member_definition(workspace, uri, &path, member) {
                    return Some(result);
                }
            }
            Type::Union(members) => {
                for member_ty in members {
                    if let Some(result) = resolve_single_member_definition(
                        workspace, uri, document, &member_ty, member,
                    ) {
                        return Some(result);
                    }
                }
            }
            _ => {}
        }
    }

    if receiver == "this" {
        if let Some(class_name) = find_enclosing_class(document, offset) {
            return class_member_definition(document, uri, &class_name, member);
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
) -> Option<Value> {
    match ty {
        Type::Named(class_name) => class_member_definition(document, uri, class_name, member),
        Type::Module(path) => module_member_definition(workspace, uri, path, member),
        Type::Union(members) => members.iter().find_map(|member_ty| {
            resolve_single_member_definition(workspace, uri, document, member_ty, member)
        }),
        _ => None,
    }
}

fn class_member_definition(
    document: &WorkspaceDocument,
    uri: &str,
    class_name: &str,
    member: &str,
) -> Option<Value> {
    let owner = if document.classes.method(class_name, member).is_some() {
        document.classes.method_owner(class_name, member)
    } else if document.classes.field(class_name, member).is_some() {
        find_field_owner(&document.classes, class_name, member)
    } else {
        None
    }?;

    let span = find_member_declaration_span(&document.text, &owner, member)?;
    Some(location(uri, document, span.0, span.1))
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
        .map(|symbol| symbol.is_exported || module_uri == current_uri)
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
            if !symbol.is_exported && module_uri != current_uri {
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
            if !symbol.is_exported && module_uri != current_uri {
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

    for kind in ["class", "interface"] {
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
        let source = r#"class Person {\n    private let name: str = \"Bruno\";\n    func greet() -> str {\n        return name;\n    }\n}\n"#;
        let span = find_member_declaration_span(source, "Person", "greet").unwrap();
        assert_eq!(&source[span.0..span.1], "greet");
    }

    #[test]
    fn finds_private_field_declaration() {
        let source = r#"class Person {\n    private let name: str = \"Bruno\";\n}\n"#;
        let span = find_member_declaration_span(source, "Person", "name").unwrap();
        assert_eq!(&source[span.0..span.1], "name");
    }

    #[test]
    fn finds_interface_method_declaration() {
        let source = r#"interface Named {\n    func name() -> str;\n}\n"#;
        let span = find_member_declaration_span(source, "Named", "name").unwrap();
        assert_eq!(&source[span.0..span.1], "name");
    }
}
