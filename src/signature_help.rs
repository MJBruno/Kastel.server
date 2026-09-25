use kastel::compiler::types::{FunctionType, Type};

use crate::module_resolver::ModuleResolver;
use crate::uri_util::{path_to_uri, uri_to_path};
use serde_json::{Value, json};

use crate::class_index::MethodInfo;
use crate::completion::{infer_receiver_types, line_and_byte_to_offset, line_text_at};
use crate::language::{
    BUILTIN_FUNCTIONS, DICT_METHODS, LIST_METHODS, RANGE_METHODS, SET_METHODS, STRING_METHODS,
    TUPLE_METHODS,
};
use crate::workspace::{Workspace, WorkspaceDocument};

#[derive(Debug, Clone)]
struct Candidate {
    label: String,
    params: Vec<String>,
    documentation: String,
}

impl Candidate {
    fn to_value(&self) -> Value {
        json!({
            "label": self.label,
            "documentation": {"kind":"markdown","value":self.documentation},
            "parameters": self.params.iter().map(|p| json!({"label":p})).collect::<Vec<_>>(),
        })
    }
}

pub fn build_signature_help(
    workspace: &Workspace,
    uri: &str,
    line: u32,
    character: u32,
) -> Option<Value> {
    let document = workspace.get(uri)?;
    let offset = line_and_byte_to_offset(
        &document.text,
        line as usize,
        utf16_byte_position(
            line_text_at(&document.text, line as usize),
            character as usize,
        ),
    );
    let call = find_active_call(&document.text, offset)?;
    let mut candidates = Vec::new();

    if let Some((name, _, doc)) = BUILTIN_FUNCTIONS
        .iter()
        .find(|(name, ..)| *name == call.callee)
    {
        let label = BUILTIN_FUNCTIONS
            .iter()
            .find(|(n, _, _)| *n == call.callee)
            .map(|(_, s, _)| s.to_string())
            .unwrap_or_else(|| format!("{}(...)", name));
        let params = parameter_labels(&label);
        candidates.push(Candidate {
            label,
            params,
            documentation: doc.to_string(),
        });
    }

    if candidates.is_empty() {
        if let Some(signatures) = document.types.function_signatures(&call.callee) {
            candidates.extend(
                signatures
                    .iter()
                    .map(|s| candidate_from_function_signature(s)),
            );
        }
    }

    if candidates.is_empty() {
        if let Some(ty) = document.types.get(&call.callee) {
            candidates.extend(type_call_candidates(document, ty));
        }
    }

    if candidates.is_empty() {
        if let Some((receiver, member)) = split_member_callee(&call.callee) {
            let receiver_offset = call.open_paren;
            for ty in infer_receiver_types(workspace, uri, document, receiver, receiver_offset) {
                candidates.extend(member_signature_candidates(
                    workspace, uri, document, &ty, member,
                ));
            }
        }
    }

    if candidates.is_empty() {
        return None;
    }

    let active_parameter = count_active_parameter(&document.text[call.open_paren + 1..offset]);
    let active_signature = candidates
        .iter()
        .position(|candidate| active_parameter < candidate.params.len())
        .unwrap_or(0);

    Some(json!({
        "signatures": candidates.iter().map(Candidate::to_value).collect::<Vec<_>>(),
        "activeSignature": active_signature,
        "activeParameter": active_parameter,
    }))
}

fn candidate_from_function_signature(signature: &crate::type_info::FunctionSignature) -> Candidate {
    Candidate {
        label: signature.label(),
        params: signature
            .params
            .iter()
            .map(|(name, ty)| format!("{}: {}", name, ty))
            .collect(),
        documentation: "Fonction Kastel".to_string(),
    }
}

fn type_call_candidates(document: &WorkspaceDocument, ty: &Type) -> Vec<Candidate> {
    match ty {
        Type::Function(function) => vec![candidate_from_function_type("call", function)],
        Type::Overloads(overloads) => overloads
            .iter()
            .map(|f| candidate_from_function_type("call", f))
            .collect(),
        Type::Named(name) => document
            .classes
            .all_methods(name)
            .iter()
            .filter(|method| method.name == "initialize")
            .map(|method| Candidate {
                label: method.signature(),
                params: method
                    .params
                    .iter()
                    .enumerate()
                    .map(|(i, n)| {
                        method
                            .param_types
                            .get(i)
                            .and_then(|t| t.as_ref())
                            .map(|t| format!("{}: {}", n, crate::class_index::type_expr_display(t)))
                            .unwrap_or_else(|| n.clone())
                    })
                    .collect(),
                documentation: "Constructeur de classe".to_string(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn member_signature_candidates(
    workspace: &Workspace,
    uri: &str,
    document: &WorkspaceDocument,
    ty: &Type,
    member: &str,
) -> Vec<Candidate> {
    match ty {
        Type::Union(members) => members
            .iter()
            .flat_map(|m| member_signature_candidates(workspace, uri, document, m, member))
            .collect(),
        Type::Module(path) => module_signature_candidates(workspace, uri, path, member),
        Type::Named(class_name) => document
            .classes
            .all_methods(class_name)
            .iter()
            .filter(|method| method.name == member)
            .map(|method| method_candidate(method))
            .collect(),
        Type::Str => table_signature(STRING_METHODS, member),
        Type::Array(_) | Type::ArrayDynamic => table_signature(LIST_METHODS, member),
        Type::Dict(_, _) | Type::DictDynamic => table_signature(DICT_METHODS, member),
        Type::Tuple(_) | Type::TupleDynamic => table_signature(TUPLE_METHODS, member),
        Type::Set(_) | Type::SetDynamic => table_signature(SET_METHODS, member),
        Type::Range => table_signature(RANGE_METHODS, member),
        _ => Vec::new(),
    }
}

fn module_signature_candidates(
    workspace: &Workspace,
    uri: &str,
    module_path: &str,
    member: &str,
) -> Vec<Candidate> {
    let Some(current_file) = uri_to_path(uri) else {
        return Vec::new();
    };
    let resolver = ModuleResolver::new(workspace.root().map(std::path::Path::to_path_buf));
    let parts = module_path
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

    if let Some(signatures) = module.types.function_signatures(member) {
        return signatures
            .iter()
            .map(candidate_from_function_signature)
            .collect();
    }

    match module.types.get(member) {
        Some(Type::Function(function)) => vec![candidate_from_function_type(member, &function)],
        Some(Type::Overloads(overloads)) => overloads
            .iter()
            .map(|function| candidate_from_function_type(member, function))
            .collect(),
        Some(Type::Named(class_name)) => module
            .classes
            .all_methods(class_name)
            .iter()
            .filter(|method| method.name == "initialize")
            .map(|method| method_candidate(method))
            .collect(),
        _ => Vec::new(),
    }
}

fn method_candidate(method: &MethodInfo) -> Candidate {
    Candidate {
        label: method.signature(),
        params: method
            .params
            .iter()
            .enumerate()
            .map(|(i, name)| {
                method
                    .param_types
                    .get(i)
                    .and_then(|t| t.as_ref())
                    .map(|t| format!("{}: {}", name, crate::class_index::type_expr_display(t)))
                    .unwrap_or_else(|| name.clone())
            })
            .collect(),
        documentation: "Méthode de classe".to_string(),
    }
}

fn table_signature(table: &[(&str, &str, &str)], member: &str) -> Vec<Candidate> {
    table
        .iter()
        .find(|(name, ..)| *name == member)
        .map(|(_, signature, doc)| {
            vec![Candidate {
                label: signature.to_string(),
                params: parameter_labels(signature),
                documentation: doc.to_string(),
            }]
        })
        .unwrap_or_default()
}

fn candidate_from_function_type(name: &str, function: &FunctionType) -> Candidate {
    let params = function
        .params
        .iter()
        .enumerate()
        .map(|(i, ty)| format!("arg{}: {}", i + 1, ty))
        .collect::<Vec<_>>();
    Candidate {
        label: format!(
            "{}({}) -> {}",
            name,
            params.join(", "),
            function.return_type
        ),
        params,
        documentation: "Fonction Kastel".to_string(),
    }
}

fn split_member_callee(callee: &str) -> Option<(&str, &str)> {
    let dot = callee.rfind('.')?;
    Some((&callee[..dot], &callee[dot + 1..]))
}

fn find_active_call(source: &str, offset: usize) -> Option<CallContext> {
    let masked = crate::text_util::mask_strings_and_comments(source);
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    let mut open = None;
    for index in (0..offset.min(bytes.len())).rev() {
        match bytes[index] {
            b')' | b']' | b'}' => depth += 1,
            b'(' => {
                if depth == 0 {
                    open = Some(index);
                    break;
                }
                depth -= 1;
            }
            b'[' | b'{' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    let open_paren = open?;
    let mut start = open_paren;
    while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric()
            || bytes[start - 1] == b'_'
            || bytes[start - 1] == b'.')
    {
        start -= 1;
    }
    let callee = source[start..open_paren].trim().to_string();
    if callee.is_empty() {
        return None;
    }
    Some(CallContext { open_paren, callee })
}

fn count_active_parameter(text: &str) -> usize {
    let masked = crate::text_util::mask_strings_and_comments(text);
    let mut depth = 0usize;
    let mut count = 0usize;
    for byte in masked.bytes() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => count += 1,
            _ => {}
        }
    }
    count
}

fn parameter_labels(signature: &str) -> Vec<String> {
    let open = match signature.find('(') {
        Some(v) => v,
        None => return Vec::new(),
    };
    let close = match signature.rfind(')') {
        Some(v) if v > open => v,
        _ => return Vec::new(),
    };
    let inner = &signature[open + 1..close];
    split_top_level(inner)
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
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

fn utf16_byte_position(line: &str, utf16: usize) -> usize {
    crate::text_util::utf16_character_to_byte_index(line, utf16)
}

struct CallContext {
    open_paren: usize,
    callee: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;

    #[test]
    fn signature_help_uses_active_parameter() {
        let mut ws = Workspace::new();
        ws.open(
            "file:///main.ks".to_string(),
            1,
            "func add(a: int, b: int) -> int { return a + b }\nadd(1, ".to_string(),
        );
        let line = 1;
        let character = 7;
        let help = build_signature_help(&ws, "file:///main.ks", line, character).unwrap();
        assert_eq!(help["activeParameter"], 1);
    }
}
