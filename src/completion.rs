use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use kastel::compiler::types::Type;
use kastel::frontend::ast::{Statement, Visibility};
use serde_json::{Value, json};

use crate::class_index::{ClassIndex, MethodInfo};
use crate::language::{
    BUILTIN_FUNCTIONS, DICT_METHODS, KEYWORDS, LIST_METHODS, RANGE_METHODS, SET_METHODS,
    STRING_METHODS, TUPLE_METHODS, TYPE_NAMES,
};
use crate::module_resolver::ModuleResolver;
use crate::symbols::{Symbol, SymbolIndex, SymbolKind};
use crate::text_util::{current_prefix, utf16_character_to_byte_index};
use crate::uri_util::{path_to_uri, uri_to_path};
use crate::workspace::{Workspace, WorkspaceDocument};

const KIND_METHOD: u32 = 2;
const KIND_FUNCTION: u32 = 3;
const KIND_FIELD: u32 = 5;
const KIND_VARIABLE: u32 = 6;
const KIND_CLASS: u32 = 7;
const KIND_INTERFACE: u32 = 8;
const KIND_MODULE: u32 = 9;
const KIND_KEYWORD: u32 = 14;
const KIND_SNIPPET: u32 = 15;
const KIND_TYPE_PARAMETER: u32 = 25;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemberContext {
    /// Expression située avant le dernier `.` : `obj`, `math`, `obj.child`.
    pub(crate) receiver: String,
    /// Préfixe tapé après le dernier `.` : `na` dans `obj.na`.
    pub(crate) prefix: String,
    pub(crate) is_import_line: bool,
}

pub fn build_completion(
    workspace: &Workspace,
    uri: &str,
    line: u32,
    character: u32,
) -> Option<Value> {
    let document = workspace.get(uri)?;
    let line_text = line_text_at(&document.text, line as usize);
    let byte_index = utf16_character_to_byte_index(line_text, character as usize);

    let mut items = Vec::new();
    let mut seen = HashSet::<String>::new();

    if let Some(context) = detect_member_access(line_text, byte_index) {
        let offset = line_and_byte_to_offset(&document.text, line as usize, byte_index);
        if context.is_import_line {
            add_import_member_completions(workspace, uri, &context, &mut items, &mut seen);
        } else {
            add_receiver_completions(
                workspace,
                uri,
                document,
                &context.receiver,
                &context.prefix,
                offset,
                &mut items,
                &mut seen,
            );
        }

        items.sort_by(|a, b| {
            a.get("sortText")
                .and_then(Value::as_str)
                .unwrap_or("")
                .cmp(b.get("sortText").and_then(Value::as_str).unwrap_or(""))
        });

        return Some(json!({
            "isIncomplete": false,
            "items": items,
        }));
    }

    let prefix = current_prefix(line_text, byte_index);

    add_keyword_completions(prefix, &mut items, &mut seen);
    add_builtin_completions(prefix, &mut items, &mut seen);
    add_type_completions(prefix, &mut items, &mut seen);
    add_document_completions(&document.symbols, prefix, false, &mut items, &mut seen);
    add_imported_name_completions(workspace, uri, document, prefix, &mut items, &mut seen);

    items.sort_by(|a, b| {
        a.get("sortText")
            .and_then(Value::as_str)
            .unwrap_or("")
            .cmp(b.get("sortText").and_then(Value::as_str).unwrap_or(""))
    });

    Some(json!({
        "isIncomplete": false,
        "items": items,
    }))
}

/// Détecte le contexte `receiver.prefix`, y compris lorsque le curseur est
/// après une partie du membre (`dog.spe`), ce que l'ancien LSP ne gérait pas.
pub(crate) fn detect_member_access(line: &str, byte_index: usize) -> Option<MemberContext> {
    let before = line.get(..byte_index)?;
    let cursor = before.trim_end_matches(char::is_whitespace);
    let member_start = cursor
        .rfind(|c: char| !is_identifier_char(c))
        .map(|index| index + 1)
        .unwrap_or(0);
    let member_prefix = &cursor[member_start..];

    let dot_region = &cursor[..member_start];
    let dot = dot_region.rfind('.')?;
    let receiver = dot_region[..dot].trim_end();
    if receiver.is_empty() {
        return None;
    }

    // Ne traite comme accès membre que les chemins d'identifiants simples.
    if !receiver.chars().all(|c| is_identifier_char(c) || c == '.') {
        return None;
    }

    Some(MemberContext {
        receiver: receiver.to_string(),
        prefix: member_prefix.to_string(),
        is_import_line: line.trim_start().starts_with("import ")
            || line.trim_start().starts_with("from "),
    })
}

pub(crate) fn line_and_byte_to_offset(text: &str, line: usize, byte_in_line: usize) -> usize {
    let mut offset = 0usize;
    for (index, raw_line) in text.split('\n').enumerate() {
        if index == line {
            return offset + byte_in_line.min(raw_line.len());
        }
        offset += raw_line.len() + 1;
    }
    text.len()
}

pub(crate) fn line_text_at(source: &str, line: usize) -> &str {
    source.split('\n').nth(line).unwrap_or("")
}

fn add_receiver_completions(
    workspace: &Workspace,
    uri: &str,
    document: &WorkspaceDocument,
    receiver_path: &str,
    prefix: &str,
    offset: usize,
    items: &mut Vec<Value>,
    seen: &mut HashSet<String>,
) {
    let receiver_types = infer_receiver_types(workspace, uri, document, receiver_path, offset);

    for ty in receiver_types {
        add_type_members(
            workspace,
            uri,
            document,
            &ty,
            receiver_path,
            prefix,
            offset,
            items,
            seen,
        );
    }

    // Un typage dynamique ne permet pas de déduire une API sûre. On conserve
    // toutefois les méthodes standard d'un conteneur lorsque le type est connu.
    if items.is_empty() {
        if let Some(name) = receiver_path.strip_prefix("this") {
            if name.is_empty() {
                if let Some(class_name) = find_enclosing_class(document, offset) {
                    add_class_members(
                        &document.classes,
                        &class_name,
                        Some(class_name.as_str()),
                        true,
                        prefix,
                        items,
                        seen,
                    );
                }
            }
        }
    }
}

fn add_type_members(
    workspace: &Workspace,
    uri: &str,
    document: &WorkspaceDocument,
    ty: &Type,
    receiver_path: &str,
    prefix: &str,
    offset: usize,
    items: &mut Vec<Value>,
    seen: &mut HashSet<String>,
) {
    match ty {
        Type::Union(members) => {
            for member in members {
                add_type_members(
                    workspace,
                    uri,
                    document,
                    member,
                    receiver_path,
                    prefix,
                    offset,
                    items,
                    seen,
                );
            }
        }
        Type::Named(class_name) => {
            let current_class = if receiver_path == "this" {
                find_enclosing_class(document, offset)
            } else {
                None
            };
            add_class_members(
                &document.classes,
                class_name,
                current_class.as_deref(),
                receiver_path == "this",
                prefix,
                items,
                seen,
            );
        }
        Type::Module(module_path) => {
            let Some(current_file) = uri_to_path(uri) else {
                return;
            };
            let resolver = ModuleResolver::new(workspace.root().map(Path::to_path_buf));
            let parts = module_path
                .split('.')
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            if let Some(path) = resolver.resolve(&current_file, &parts) {
                let module_uri = path_to_uri(&path);
                if let Some(module) = workspace.get(&module_uri) {
                    add_document_completions(&module.symbols, prefix, true, items, seen);
                }
            }
        }
        Type::Array(_) | Type::ArrayDynamic => {
            add_table_members(LIST_METHODS, prefix, KIND_METHOD, items, seen);
        }
        Type::Dict(_, _) | Type::DictDynamic => {
            add_table_members(DICT_METHODS, prefix, KIND_METHOD, items, seen);
        }
        Type::Tuple(_) | Type::TupleDynamic => {
            add_table_members(TUPLE_METHODS, prefix, KIND_METHOD, items, seen);
        }
        Type::Set(_) | Type::SetDynamic => {
            add_table_members(SET_METHODS, prefix, KIND_METHOD, items, seen);
        }
        Type::Str => {
            add_table_members(STRING_METHODS, prefix, KIND_METHOD, items, seen);
        }
        Type::Range => {
            add_table_members(RANGE_METHODS, prefix, KIND_METHOD, items, seen);
        }
        Type::Record(fields) => {
            for (name, field_type) in fields {
                if !name.starts_with(prefix) || !seen.insert(name.clone()) {
                    continue;
                }
                items.push(json!({
                    "label": name,
                    "kind": KIND_FIELD,
                    "detail": format!("{}: {}", name, field_type),
                    "sortText": format!("0_{}", name),
                }));
            }
            for name in ["keys", "values", "entries", "copy", "to_string"] {
                if name.starts_with(prefix) && seen.insert(name.to_string()) {
                    items.push(json!({
                        "label": name,
                        "kind": KIND_METHOD,
                        "detail": "Record method",
                        "insertText": format!("{}($0)", name),
                        "insertTextFormat": 2,
                        "sortText": format!("2_{}", name),
                    }));
                }
            }
        }
        _ => {}
    }

    // `offset` est utilisé comme argument de résolution du type de `this`;
    // conserver le paramètre dans l'API rend la fonction extensible pour les
    // futures scopes locales.
    let _ = offset;
}

pub(crate) fn infer_receiver_types(
    workspace: &Workspace,
    uri: &str,
    document: &WorkspaceDocument,
    receiver_path: &str,
    offset: usize,
) -> Vec<Type> {
    if receiver_path == "this" {
        return find_enclosing_class(document, offset)
            .and_then(|name| Some(vec![Type::Named(name)]))
            .unwrap_or_default();
    }

    if receiver_path == "base" {
        let Some(class_name) = find_enclosing_class(document, offset) else {
            return Vec::new();
        };
        let Some(info) = document.classes.get(&class_name) else {
            return Vec::new();
        };
        return info.bases.iter().cloned().map(Type::Named).collect();
    }

    if let Some(first) = receiver_path.split('.').next() {
        if let Some(ty) = document.types.get(first) {
            let mut current = vec![ty.clone()];
            for segment in receiver_path.split('.').skip(1) {
                let mut next = Vec::new();
                for ty in current {
                    next.extend(member_result_types(workspace, uri, document, &ty, segment));
                }
                current = next;
            }
            if !current.is_empty() {
                return current;
            }
        }
    }

    // Paramètres et variables locales ne sont pas des symboles globaux dans
    // TypeInfo. Les récupérer directement dans le texte permet à l'IntelliSense
    // de rester fonctionnel pendant l'édition d'une fonction/méthode.
    if let Some(ty) = infer_local_type(document, receiver_path, offset) {
        return vec![ty];
    }

    // `new Class(...)` sans annotation : le TypeInfo courant devrait déjà le
    // déduire. Ce repli sert uniquement aux sources partielles.
    if let Some(class_name) =
        infer_class_of_variable(&document.text, receiver_path, &document.classes)
    {
        return vec![Type::Named(class_name)];
    }

    // Résolution d'un alias de module simple.
    if let Some(import) = parse_import_bindings(&document.text)
        .into_iter()
        .find(|i| i.local == receiver_path)
    {
        if let Some(current_file) = uri_to_path(uri) {
            let resolver = ModuleResolver::new(workspace.root().map(Path::to_path_buf));
            if let Some(module_path) = resolver.resolve(&current_file, &import.parts) {
                return vec![Type::Module(module_path.to_string_lossy().to_string())];
            }
        }
    }

    Vec::new()
}

fn infer_local_type(document: &WorkspaceDocument, name: &str, offset: usize) -> Option<Type> {
    if let Some(ty) = infer_parameter_type(&document.text, name, offset) {
        return Some(ty);
    }

    let masked = crate::text_util::mask_strings_and_comments(&document.text);
    let prefix = &masked[..offset.min(masked.len())];
    let needle = format!("let {}", name);
    let mut search_end = prefix.len();
    let mut last = None;

    while let Some(relative) = prefix[..search_end].rfind(&needle) {
        let start = relative;
        let name_start = start + 4;
        if crate::text_util::is_identifier_boundary(prefix, name_start, name_start + name.len()) {
            last = Some(start);
            break;
        }
        if start == 0 {
            break;
        }
        search_end = start;
    }

    let start = last?;
    let tail_end = prefix[start..]
        .find('\n')
        .map(|relative| start + relative)
        .unwrap_or(prefix.len());
    let declaration = &prefix[start..tail_end];

    if let Some(colon) = declaration.find(':') {
        let type_text = declaration[colon + 1..]
            .split_once('=')
            .map(|(value, _)| value)
            .unwrap_or(&declaration[colon + 1..])
            .trim();
        if let Some(ty) = parse_type_text(type_text) {
            return Some(ty);
        }
    }

    if let Some(new_pos) = declaration.find("new ") {
        let after = &declaration[new_pos + 4..];
        let class_name = after
            .split(|c: char| c == '(' || c.is_whitespace() || c == ';')
            .next()
            .unwrap_or("");
        if !class_name.is_empty() && document.classes.contains(class_name) {
            return Some(Type::Named(class_name.to_string()));
        }
    }

    if let Some(eq) = declaration.find('=') {
        let expression = declaration[eq + 1..].trim();
        if let Some(first) = expression
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .find(|part| !part.is_empty())
        {
            if let Some(ty) = document.types.get(first) {
                return Some(ty.clone());
            }
        }
    }

    None
}

fn infer_parameter_type(source: &str, name: &str, offset: usize) -> Option<Type> {
    let masked = crate::text_util::mask_strings_and_comments(source);
    let bytes = masked.as_bytes();
    let mut search = 0usize;
    let mut best: Option<(usize, Type)> = None;

    while let Some(relative) = masked[search..].find("func ") {
        let func_start = search + relative;
        let name_start = func_start + 5;
        let Some(paren_rel) = masked[name_start..].find('(') else {
            break;
        };
        let open = name_start + paren_rel;
        let declared_name = masked[name_start..open].trim();
        if declared_name.is_empty() {
            search = open + 1;
            continue;
        }
        let Some(close) = find_matching_delimiter(bytes, open, b'(', b')') else {
            break;
        };
        let Some(brace_rel) = masked[close + 1..].find('{') else {
            break;
        };
        let brace = close + 1 + brace_rel;
        let Some(body_end) = find_matching_delimiter(bytes, brace, b'{', b'}') else {
            break;
        };
        if offset > brace && offset <= body_end {
            if let Some(ty) = parameter_type_in_list(source, open + 1, close, name) {
                let span_len = body_end - brace;
                if best
                    .as_ref()
                    .map(|(length, _)| span_len < *length)
                    .unwrap_or(true)
                {
                    best = Some((span_len, ty));
                }
            }
        }
        search = body_end.saturating_add(1);
    }

    best.map(|(_, ty)| ty)
}

fn parameter_type_in_list(source: &str, start: usize, end: usize, name: &str) -> Option<Type> {
    let text = &source[start..end];
    let masked = crate::text_util::mask_strings_and_comments(text);
    for segment in split_top_level(&masked) {
        let segment_start = masked.find(&segment)?;
        let segment_source = &text[segment_start..segment_start + segment.len()];
        let trimmed = segment_source.trim();
        let param_name = trimmed
            .split(|c: char| c == ':' || c == '=' || c.is_whitespace())
            .next()
            .unwrap_or("");
        if param_name != name {
            continue;
        }
        let colon = trimmed.find(':')?;
        let type_text = trimmed[colon + 1..]
            .split_once('=')
            .map(|(value, _)| value)
            .unwrap_or(&trimmed[colon + 1..])
            .trim();
        return parse_type_text(type_text);
    }
    None
}

fn split_top_level(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'<' | b'(' | b'[' | b'{' => depth += 1,
            b'>' | b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                parts.push(text[start..index].to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].to_string());
    parts
}

fn find_matching_delimiter(source: &[u8], open: usize, opening: u8, closing: u8) -> Option<usize> {
    let mut depth = 0usize;
    for index in open..source.len() {
        match source[index] {
            value if value == opening => depth += 1,
            value if value == closing => {
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

fn parse_type_text(text: &str) -> Option<Type> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    let union = split_type_union(text);
    if union.len() > 1 {
        return Some(Type::union_of(
            union
                .into_iter()
                .filter_map(|part| parse_type_text(&part))
                .collect(),
        ));
    }

    if let Some(open) = text.find('<') {
        if text.ends_with('>') {
            let name = text[..open].trim();
            let args = split_generic_arguments(&text[open + 1..text.len() - 1])
                .into_iter()
                .filter_map(|arg| parse_type_text(&arg))
                .collect::<Vec<_>>();
            return match (name, args.as_slice()) {
                ("List", [element]) => Some(Type::Array(Box::new(element.clone()))),
                ("Dict", [key, value]) => {
                    Some(Type::Dict(Box::new(key.clone()), Box::new(value.clone())))
                }
                ("Tuple", _) => Some(Type::Tuple(args)),
                ("Set", [element]) => Some(Type::Set(Box::new(element.clone()))),
                _ => Some(Type::Generic {
                    name: name.to_string(),
                    arguments: args,
                }),
            };
        }
    }

    Some(match text {
        "int" => Type::Int,
        "float" => Type::Float,
        "str" => Type::Str,
        "bool" => Type::Bool,
        "None" => Type::None,
        "dynamic" | "any" => Type::Dynamic,
        "Range" => Type::Range,
        "List" => Type::ArrayDynamic,
        "Dict" => Type::DictDynamic,
        "Tuple" => Type::TupleDynamic,
        "Set" => Type::SetDynamic,
        other => Type::Named(other.to_string()),
    })
}

fn split_type_union(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'<' | b'(' | b'[' | b'{' => depth += 1,
            b'>' | b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'|' if depth == 0 => {
                parts.push(text[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim().to_string());
    parts
}

fn split_generic_arguments(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'<' | b'(' | b'[' | b'{' => depth += 1,
            b'>' | b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                parts.push(text[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim().to_string());
    parts
}

fn member_result_types(
    workspace: &Workspace,
    uri: &str,
    document: &WorkspaceDocument,
    ty: &Type,
    name: &str,
) -> Vec<Type> {
    match ty {
        Type::Union(members) => members
            .iter()
            .flat_map(|member| member_result_types(workspace, uri, document, member, name))
            .collect(),
        Type::Named(class_name) => {
            if let Some(field) = document.classes.field(class_name, name) {
                if let Some(annotation) = &field.type_annotation {
                    return vec![Type::from_type_expr(annotation)];
                }
            }
            if let Some(method) = document.classes.method(class_name, name) {
                return vec![Type::Function(method.function_type())];
            }
            Vec::new()
        }
        Type::Module(path) => {
            let Some(current_file) = uri_to_path(uri) else {
                return Vec::new();
            };
            let resolver = ModuleResolver::new(workspace.root().map(Path::to_path_buf));
            let parts = path
                .split('.')
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            let Some(module_path) = resolver.resolve(&current_file, &parts) else {
                return Vec::new();
            };
            let module_uri = path_to_uri(&module_path);
            let Some(module) = workspace.get(&module_uri) else {
                return Vec::new();
            };
            module
                .types
                .get(name)
                .cloned()
                .map(|ty| vec![ty])
                .unwrap_or_default()
        }
        _ => document
            .types
            .member_type(ty, name)
            .map(|value| vec![value])
            .unwrap_or_default(),
    }
}

fn add_class_members(
    classes: &ClassIndex,
    class_name: &str,
    current_class: Option<&str>,
    allow_private: bool,
    prefix: &str,
    items: &mut Vec<Value>,
    seen: &mut HashSet<String>,
) {
    for field_name in classes.all_fields(class_name) {
        let visible = classes
            .field(class_name, field_name)
            .map(|field| match field.visibility {
                Visibility::Public => true,
                Visibility::Private => {
                    allow_private
                        && classes.field_owner(class_name, field_name).as_deref() == current_class
                }
            })
            .unwrap_or(true);
        if !visible || !field_name.starts_with(prefix) || !seen.insert(field_name.to_string()) {
            continue;
        }
        let detail = classes
            .field(class_name, field_name)
            .and_then(|field| field.type_annotation.as_ref())
            .map(crate::class_index::type_expr_display)
            .map(|ty| format!("{}: {}", field_name, ty))
            .unwrap_or_else(|| field_name.to_string());
        items.push(json!({
            "label": field_name,
            "kind": KIND_FIELD,
            "detail": detail,
            "sortText": format!("0_{}", field_name),
        }));
    }

    for method in classes.all_methods(class_name) {
        if method.visibility == Visibility::Private {
            let owner_is_current =
                classes.method_owner(class_name, &method.name).as_deref() == current_class;
            if !allow_private || !owner_is_current {
                continue;
            }
        }
        if !method.name.starts_with(prefix) || !seen.insert(method.name.clone()) {
            continue;
        }
        items.push(method_completion(method, "1_"));
    }
}

fn method_completion(method: &MethodInfo, sort_prefix: &str) -> Value {
    json!({
        "label": method.name,
        "kind": KIND_METHOD,
        "detail": method.signature(),
        "insertText": format!("{}($0)", method.name),
        "insertTextFormat": 2,
        "documentation": {
            "kind": "markdown",
            "value": format!("Méthode de classe{}.", if method.visibility == Visibility::Private { " privée" } else { "" })
        },
        "sortText": format!("{}{}", sort_prefix, method.name),
    })
}

fn add_import_member_completions(
    workspace: &Workspace,
    uri: &str,
    context: &MemberContext,
    items: &mut Vec<Value>,
    seen: &mut HashSet<String>,
) {
    let Some(document) = workspace.get(uri) else {
        return;
    };

    if let Some(module_path) = resolve_module_path(workspace, uri, &context.receiver) {
        let module_uri = path_to_uri(&module_path);
        if let Some(module) = workspace.get(&module_uri) {
            add_document_completions(&module.symbols, &context.prefix, true, items, seen);
            return;
        }
    }

    // `import math.` / `from math import ...` : proposer les sous-modules.
    if let Some(current_file) = uri_to_path(uri) {
        let path_parts = context
            .receiver
            .split('.')
            .map(str::to_string)
            .collect::<Vec<_>>();
        let names = list_submodules(workspace.root(), &current_file, &path_parts);
        for name in names {
            if !name.starts_with(&context.prefix) || !seen.insert(name.clone()) {
                continue;
            }
            items.push(json!({
                "label": name,
                "kind": KIND_MODULE,
                "detail": "Kastel module",
                "sortText": format!("0_{}", name),
            }));
        }
    }

    let _ = document;
}

fn add_imported_name_completions(
    workspace: &Workspace,
    uri: &str,
    document: &WorkspaceDocument,
    prefix: &str,
    items: &mut Vec<Value>,
    seen: &mut HashSet<String>,
) {
    let Some(current_file) = uri_to_path(uri) else {
        return;
    };
    let resolver = ModuleResolver::new(workspace.root().map(Path::to_path_buf));

    for import in parse_import_bindings(&document.text) {
        let Some(module_path) = resolver.resolve(&current_file, &import.parts) else {
            continue;
        };
        let module_uri = path_to_uri(&module_path);
        let Some(module) = workspace.get(&module_uri) else {
            continue;
        };
        add_document_completions(&module.symbols, prefix, true, items, seen);
    }
}

fn resolve_module_path(workspace: &Workspace, uri: &str, receiver: &str) -> Option<PathBuf> {
    let current_file = uri_to_path(uri)?;
    let resolver = ModuleResolver::new(workspace.root().map(Path::to_path_buf));
    let imports = parse_import_bindings(&workspace.get(uri)?.text);

    for import in imports {
        let full = import.parts.join(".");
        if receiver == full || receiver == import.local {
            return resolver.resolve(&current_file, &import.parts);
        }
    }

    resolver.resolve(
        &current_file,
        &receiver.split('.').map(str::to_string).collect::<Vec<_>>(),
    )
}

#[derive(Debug, Clone)]
struct ImportBinding {
    parts: Vec<String>,
    local: String,
}

fn parse_import_bindings(source: &str) -> Vec<ImportBinding> {
    let mut imports = Vec::new();
    for raw_line in source.lines() {
        let line = raw_line.trim().trim_end_matches(';');
        if let Some(rest) = line.strip_prefix("import ") {
            let rest = rest.trim();
            if rest.starts_with('{') || rest.starts_with('*') {
                continue;
            }
            let parts = rest
                .split('.')
                .map(str::to_string)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            if let Some(last) = parts.last() {
                imports.push(ImportBinding {
                    local: last.clone(),
                    parts,
                });
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("from ") {
            if let Some((module, names)) = rest.split_once(" import ") {
                let parts = module
                    .trim()
                    .split('.')
                    .map(str::to_string)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                for item in names.trim().trim_matches(['{', '}']).split(',') {
                    let mut p = item.trim().split_whitespace();
                    let Some(name) = p.next() else { continue };
                    let local = if p.next() == Some("as") {
                        p.next().unwrap_or(name)
                    } else {
                        name
                    };
                    imports.push(ImportBinding {
                        local: local.to_string(),
                        parts: parts.clone(),
                    });
                }
            }
        }
    }
    imports
}

fn add_table_members(
    table: &[(&str, &str, &str)],
    prefix: &str,
    kind: u32,
    items: &mut Vec<Value>,
    seen: &mut HashSet<String>,
) {
    for (name, signature, doc) in table {
        if !name.starts_with(prefix) || !seen.insert((*name).to_string()) {
            continue;
        }
        items.push(json!({
            "label": *name,
            "kind": kind,
            "detail": *signature,
            "documentation": { "kind": "markdown", "value": *doc },
            "insertText": format!("{}($0)", name),
            "insertTextFormat": 2,
            "sortText": format!("1_{}", name),
        }));
    }
}

fn add_keyword_completions(prefix: &str, items: &mut Vec<Value>, seen: &mut HashSet<String>) {
    for keyword in KEYWORDS {
        if !keyword.starts_with(prefix) || !seen.insert((*keyword).to_string()) {
            continue;
        }
        if let Some(snippet) = keyword_snippet(keyword) {
            items.push(json!({
                "label": keyword,
                "kind": KIND_SNIPPET,
                "detail": "Kastel snippet",
                "insertText": snippet,
                "insertTextFormat": 2,
                "sortText": format!("3_{}", keyword),
            }));
        } else {
            items.push(json!({
                "label": keyword,
                "kind": KIND_KEYWORD,
                "detail": "Kastel keyword",
                "sortText": format!("4_{}", keyword),
            }));
        }
    }
}

fn keyword_snippet(keyword: &str) -> Option<&'static str> {
    Some(match keyword {
        "const" => "const ${1:NAME} = ${0}",
        "let" => "let ${1:name} = ${0}",
        "func" => "func ${1:name}(${2}) -> ${3:dynamic} {\n\t${0}\n}",
        "class" => "class ${1:Name} {\n\t${0}\n}",
        "interface" => "interface ${1:Name} {\n\t${0}\n}",
        "if" => "if ${1:condition} {\n\t${0}\n}",
        "else" => "else {\n\t${0}\n}",
        "while" => "while ${1:condition} {\n\t${0}\n}",
        "for" => "for ${1:item} in ${2:collection} {\n\t${0}\n}",
        "import" => "import ${1:module.path}",
        "from" => "from ${1:module} import ${2:name}",
        "export" => "export ${0}",
        "try" => "try {\n\t${0}\n} catch ${1:error} {\n\t\n}",
        "catch" => "catch ${1:error} {\n\t${0}\n}",
        "finally" => "finally {\n\t${0}\n}",
        "throw" => "throw ${0}",
        "new" => "new ${1:ClassName}(${0})",
        "match" => "match ${1:value} {\n\t${0}\n}",
        "return" => "return ${0}",
        _ => return None,
    })
}

fn add_builtin_completions(prefix: &str, items: &mut Vec<Value>, seen: &mut HashSet<String>) {
    for (name, signature, doc) in BUILTIN_FUNCTIONS {
        if !name.starts_with(prefix) || !seen.insert((*name).to_string()) {
            continue;
        }
        items.push(json!({
            "label": *name,
            "kind": KIND_FUNCTION,
            "detail": *signature,
            "documentation": { "kind": "markdown", "value": *doc },
            "insertText": format!("{}($0)", name),
            "insertTextFormat": 2,
            "sortText": format!("1_{}", name),
        }));
    }
}

fn add_type_completions(prefix: &str, items: &mut Vec<Value>, seen: &mut HashSet<String>) {
    for name in TYPE_NAMES {
        if !name.starts_with(prefix) || !seen.insert((*name).to_string()) {
            continue;
        }
        items.push(json!({
            "label": *name,
            "kind": KIND_TYPE_PARAMETER,
            "detail": "Kastel type",
            "sortText": format!("2_{}", name),
        }));
    }
}

fn add_document_completions(
    symbols: &SymbolIndex,
    prefix: &str,
    imported: bool,
    items: &mut Vec<Value>,
    seen: &mut HashSet<String>,
) {
    for symbol in symbols.iter() {
        if imported && !symbol.is_exported {
            continue;
        }
        if !symbol.name.starts_with(prefix) || !seen.insert(symbol.name.clone()) {
            continue;
        }
        items.push(symbol_completion(symbol, imported));
    }
}

fn symbol_completion(symbol: &Symbol, imported: bool) -> Value {
    let kind = match symbol.kind {
        SymbolKind::Variable => KIND_VARIABLE,
        SymbolKind::Function => KIND_FUNCTION,
        SymbolKind::Class => KIND_CLASS,
        SymbolKind::Interface => KIND_INTERFACE,
        SymbolKind::Import => KIND_MODULE,
        SymbolKind::TypeAlias => KIND_TYPE_PARAMETER,
    };
    let detail = if let Some(signature) = &symbol.signature {
        signature.clone()
    } else if let Some(ty) = &symbol.type_display {
        match symbol.kind {
            SymbolKind::Class | SymbolKind::Interface => ty.clone(),
            _ => format!("{}: {}", symbol.name, ty),
        }
    } else {
        match symbol.kind {
            SymbolKind::Variable => "variable".to_string(),
            SymbolKind::Function => "function".to_string(),
            SymbolKind::Class => "class".to_string(),
            SymbolKind::Interface => "interface".to_string(),
            SymbolKind::Import => "module".to_string(),
            SymbolKind::TypeAlias => "type alias".to_string(),
        }
    };
    json!({
        "label": symbol.name,
        "kind": kind,
        "detail": if imported { format!("Imported {}", detail) } else { detail },
        "insertText": symbol.name,
        "sortText": format!("5_{}", symbol.name),
    })
}

fn find_enclosing_class(document: &WorkspaceDocument, offset: usize) -> Option<String> {
    fn visit(statements: &[Statement], source: &str, offset: usize, current: &mut Option<String>) {
        for raw in statements {
            match raw {
                Statement::Positioned { statement, .. } => visit(
                    std::slice::from_ref(statement.as_ref()),
                    source,
                    offset,
                    current,
                ),
                Statement::Export { statement } => visit(
                    std::slice::from_ref(statement.as_ref()),
                    source,
                    offset,
                    current,
                ),
                Statement::Class { name, .. } => {
                    if let Some(span) = find_class_span(source, name) {
                        if span.0 <= offset && offset <= span.1 {
                            *current = Some(name.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut current = None;
    visit(&document.statements, &document.text, offset, &mut current);
    current
}

fn find_class_span(source: &str, name: &str) -> Option<(usize, usize)> {
    let masked = crate::text_util::mask_strings_and_comments(source);
    let needle = format!("class {}", name);
    let start = masked.find(&needle)?;
    let brace = masked[start..].find('{')? + start;
    let end = find_matching_brace(source, brace)?;
    Some((start, end))
}

fn find_matching_brace(source: &str, open: usize) -> Option<usize> {
    let masked = crate::text_util::mask_strings_and_comments(source);
    let bytes = masked.as_bytes();
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

fn infer_class_of_variable(source: &str, variable: &str, classes: &ClassIndex) -> Option<String> {
    let needle = format!("let {}", variable);
    let mut search = 0usize;
    while let Some(relative) = source[search..].find(&needle) {
        let start = search + relative;
        let end = start + needle.len();
        if !crate::text_util::is_identifier_boundary(source, start + 4, end) {
            search = end;
            continue;
        }
        let tail = &source[end..source.len().min(end + 180)];
        if let Some(annotation) = tail.strip_prefix(":") {
            let annotation = annotation.trim_start();
            if let Some(name) = annotation
                .split(|c: char| c == '=' || c.is_whitespace())
                .next()
            {
                if !name.is_empty() && classes.contains(name) {
                    return Some(name.to_string());
                }
            }
        }
        if let Some(new_pos) = tail.find("new ") {
            let after = &tail[new_pos + 4..];
            let name = after
                .split(|c: char| c == '(' || c.is_whitespace())
                .next()
                .unwrap_or("");
            if classes.contains(name) {
                return Some(name.to_string());
            }
        }
        search = end;
    }
    None
}

fn list_submodules(root: Option<&Path>, current_file: &Path, path_parts: &[String]) -> Vec<String> {
    let mut candidates = Vec::new();
    let bases = [
        current_file.parent().map(PathBuf::from),
        root.map(PathBuf::from),
    ];

    for base in bases.into_iter().flatten() {
        let mut dir = base;
        for part in path_parts {
            if !is_identifier_char_string(part) {
                dir = PathBuf::new();
                break;
            }
            dir.push(part);
        }
        if dir.as_os_str().is_empty() || !dir.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    candidates.push(name.to_string());
                }
            } else if path.extension().and_then(|s| s.to_str()) == Some("ks") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    candidates.push(name.to_string());
                }
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn is_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
fn is_identifier_char_string(s: &str) -> bool {
    !s.is_empty() && s.chars().all(is_identifier_char)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_partial_member_access() {
        let ctx = detect_member_access("value.si", 8).unwrap();
        assert_eq!(ctx.receiver, "value");
        assert_eq!(ctx.prefix, "si");
    }

    #[test]
    fn detects_nested_member_access() {
        let ctx = detect_member_access("math.core.Co", 13).unwrap();
        assert_eq!(ctx.receiver, "math.core");
        assert_eq!(ctx.prefix, "Co");
    }

    #[test]
    fn list_is_current_container_name() {
        assert!(LIST_METHODS.iter().any(|(name, _, _)| *name == "add"));
        assert!(!LIST_METHODS.iter().any(|(name, _, _)| *name == "push"));
    }
}
