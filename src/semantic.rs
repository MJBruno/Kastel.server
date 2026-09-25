//! Analyse sémantique légère du LSP Kastel.
//!
//! L'analyse vérifie qu'un identifiant utilisé est soit :
//! - un mot-clé ou un builtin ;
//! - un symbole global du workspace ;
//! - un paramètre de fonction actuellement visible ;
//! - une variable locale actuellement visible ;
//! - une variable de boucle / de catch actuellement visible.
//!
//! Cette analyse reste volontairement textuelle : le parser Kastel est déjà
//! utilisé ailleurs dans le LSP pour construire les index de symboles/classes.

use std::collections::{HashMap, HashSet};

use crate::analyzer::Diagnostic;
use crate::language::{BUILTINS, CONTEXTUAL_KEYWORDS, KEYWORDS, TYPE_NAMES};
use crate::text_util::{is_identifier_byte, mask_strings_and_comments};
use crate::workspace::Workspace;

struct Ident {
    name: String,
    offset: usize,
}

#[derive(Debug, Clone)]
enum LexTokenKind {
    Identifier(String),
    Punctuation(u8),
}

#[derive(Debug, Clone)]
struct LexToken {
    kind: LexTokenKind,
    start: usize,
}

#[derive(Debug, Default)]
struct Scope {
    start: usize,
    end: usize,
    parent: Option<usize>,
    names: HashSet<String>,
}

/// Binding local dont la portée est explicite en offsets.
///
/// C'est particulièrement important pour les paramètres : leur déclaration
/// se trouve avant le corps de la fonction, alors que toutes leurs utilisations
/// se trouvent dans ce corps et dans les blocs imbriqués.
#[derive(Debug, Clone)]
struct Binding {
    name: String,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone)]
struct Parameter {
    name: String,
    start: usize,
}

/// Analyse les identifiants du document `uri`.
pub fn analyze(workspace: &Workspace, uri: &str) -> Vec<Diagnostic> {
    let Some(document) = workspace.get(uri) else {
        return Vec::new();
    };

    let masked = mask_strings_and_comments(&document.text);
    let known = collect_known_names(workspace);
    let tokens = scan_tokens(&masked);

    let (mut scopes, brace_scopes) = build_scopes(&masked);
    let mut bindings = Vec::new();

    collect_local_declarations(&tokens, &brace_scopes, &mut scopes);

    collect_function_parameter_bindings(&masked, &tokens, &scopes, &mut bindings);

    let identifiers = scan_identifiers(&masked);
    let mut diagnostics = Vec::new();

    for ident in identifiers {
        if should_skip_identifier(&masked, &ident) {
            continue;
        }

        if is_resolved(&ident.name, ident.offset, &scopes, &bindings, &known) {
            continue;
        }

        let (line, column) = offset_to_position(&document.text, ident.offset);

        diagnostics.push(Diagnostic::error(
            format!("undefined identifier: {}", ident.name),
            line,
            column,
            "undefined-identifier",
        ));
    }

    diagnostics
}

/// Les noms déclarés au niveau racine sont visibles dans les autres
/// documents du workspace. Les variables locales ne le sont pas.
fn collect_known_names(workspace: &Workspace) -> HashSet<String> {
    let mut known = HashSet::new();

    for (_, document) in workspace.iter() {
        let masked = mask_strings_and_comments(&document.text);

        for symbol in document.symbols.iter() {
            if brace_depth_at(&masked, symbol.span.start) == 0 {
                known.insert(symbol.name.clone());
            }
        }
    }

    known
}

fn should_skip_identifier(source: &str, ident: &Ident) -> bool {
    let name = ident.name.as_str();

    // `_` is Kastel's wildcard pattern and is not a variable reference.
    // It must not produce a false-positive undefined-identifier diagnostic.
    if name == "_" {
        return true;
    }

    if KEYWORDS.contains(&name) {
        return true;
    }

    if BUILTINS.contains(&name) {
        return true;
    }

    // Modificateurs contextuels (`public`, `private`) et noms de types
    // intégrés ne sont pas des références runtime.
    if CONTEXTUAL_KEYWORDS.contains(&name) || TYPE_NAMES.contains(&name) {
        return true;
    }

    // Un chemin d'import (`import std.math`, `from dog import Dog`)
    // n'est pas une suite de références runtime.
    if is_import_line(source, ident.offset) {
        return true;
    }

    // Le nom immédiatement après une déclaration n'est pas une référence.
    if is_declaration_name(source, ident.offset) {
        return true;
    }

    // `object.member` : le membre ne doit pas être recherché comme variable.
    if is_member_access(source, ident.offset) {
        return true;
    }

    // Les clés de littéraux objet et les champs de types structurés ne sont
    // pas des références de variables.
    //
    //     { nom: "Mahasolo", age: 28 }
    //     type Users = { nom: str, age: int }
    //
    // Dans les deux cas, l'identifiant est suivi immédiatement de `:`.
    if is_object_key_or_field(source, ident.offset) {
        return true;
    }

    false
}

#[allow(clippy::skip_while_next)]
fn is_object_key_or_field(source: &str, offset: usize) -> bool {
    let mut chars = source[offset..].chars();

    // Saute le nom de l'identifiant courant.
    while let Some(c) = chars.next() {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            // Puis les espaces éventuels avant `:`.
            if c.is_whitespace()
                && chars.skip_while(|next| next.is_whitespace()).next() == Some(':')
            {
                return true;
            }
            return c == ':';
        }
    }

    false
}

fn is_member_access(source: &str, offset: usize) -> bool {
    source[..offset].trim_end().ends_with('.')
}

fn is_import_line(source: &str, offset: usize) -> bool {
    let line_start = source[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let line = source[line_start..]
        .split('\n')
        .next()
        .unwrap_or("")
        .trim_start();
    line.starts_with("import ") || line.starts_with("from ")
}

fn is_declaration_name(source: &str, offset: usize) -> bool {
    let line_start = source[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let prefix = &source[line_start..offset];
    let mut previous = None;
    for part in prefix.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if !part.is_empty() {
            previous = Some(part);
        }
    }
    matches!(
        previous,
        Some("func" | "class" | "interface" | "type" | "let" | "const")
    )
}

fn is_resolved(
    name: &str,
    offset: usize,
    scopes: &[Scope],
    bindings: &[Binding],
    known: &HashSet<String>,
) -> bool {
    if known.contains(name) {
        return true;
    }

    // Résolution directe des paramètres / bindings à portée explicite.
    if bindings
        .iter()
        .any(|binding| binding.name == name && binding.start <= offset && offset < binding.end)
    {
        return true;
    }

    // Résolution des variables locales par chaîne de scopes.
    let mut scope = innermost_scope_at(scopes, offset);

    loop {
        if scopes[scope].names.contains(name) {
            return true;
        }

        match scopes[scope].parent {
            Some(parent) => scope = parent,
            None => break,
        }
    }

    false
}

/// Construit les scopes lexicaux à partir des accolades du code réel.
fn build_scopes(source: &str) -> (Vec<Scope>, HashMap<usize, usize>) {
    let mut scopes = vec![Scope {
        start: 0,
        end: source.len(),
        parent: None,
        names: HashSet::new(),
    }];

    let mut brace_stack = vec![0usize];
    let mut brace_scopes = HashMap::new();

    for (offset, byte) in source.bytes().enumerate() {
        match byte {
            b'{' => {
                let parent = *brace_stack.last().unwrap_or(&0);
                let scope_id = scopes.len();

                scopes.push(Scope {
                    start: offset + 1,
                    end: source.len(),
                    parent: Some(parent),
                    names: HashSet::new(),
                });

                brace_scopes.insert(offset, scope_id);
                brace_stack.push(scope_id);
            }

            b'}' if brace_stack.len() > 1 => {
                let scope_id = brace_stack.pop().unwrap();
                scopes[scope_id].end = offset;
            }

            _ => {}
        }
    }

    (scopes, brace_scopes)
}

fn collect_local_declarations(
    tokens: &[LexToken],
    brace_scopes: &HashMap<usize, usize>,
    scopes: &mut [Scope],
) {
    let mut index = 0usize;

    while index < tokens.len() {
        let Some(keyword) = token_identifier(tokens, index) else {
            index += 1;
            continue;
        };

        match keyword {
            "let" | "const" => {
                if let Some(name_index) = next_identifier_index(tokens, index + 1)
                    && let Some(name) = token_identifier(tokens, name_index)
                {
                    let scope = innermost_scope_at(scopes, tokens[name_index].start);
                    scopes[scope].names.insert(name.to_string());
                }
            }

            "for" => {
                collect_for_variable(tokens, index, brace_scopes, scopes);
            }

            "catch" => {
                collect_catch_parameter(tokens, index, brace_scopes, scopes);
            }

            "func" => {
                collect_function_name(tokens, index, scopes);
            }

            _ => {}
        }

        index += 1;
    }
}

fn collect_function_name(tokens: &[LexToken], func_index: usize, scopes: &mut [Scope]) {
    let Some(name_index) = next_identifier_index(tokens, func_index + 1) else {
        return;
    };

    let Some(name) = token_identifier(tokens, name_index) else {
        return;
    };

    let scope = innermost_scope_at(scopes, tokens[func_index].start);
    scopes[scope].names.insert(name.to_string());
}

/// Collecte les paramètres via leur vraie zone source puis leur associe la
/// portée du corps de fonction. Cette résolution ne dépend donc pas du scope
/// choisi à l'emplacement de l'utilisation du paramètre.
fn collect_function_parameter_bindings(
    source: &str,
    tokens: &[LexToken],
    scopes: &[Scope],
    bindings: &mut Vec<Binding>,
) {
    let mut index = 0usize;

    while index < tokens.len() {
        if token_identifier(tokens, index) != Some("func") {
            index += 1;
            continue;
        }

        let Some(name_index) = next_identifier_index(tokens, index + 1) else {
            index += 1;
            continue;
        };

        let Some(open_paren_index) = next_punctuation_index(tokens, name_index + 1, b'(') else {
            index = name_index + 1;
            continue;
        };

        let open_paren = tokens[open_paren_index].start;
        let Some(close_paren) = find_matching_char(source, open_paren, b'(', b')') else {
            index = open_paren_index + 1;
            continue;
        };

        let Some(open_brace) = find_next_open_brace(source, close_paren) else {
            index = open_paren_index + 1;
            continue;
        };

        let Some(body_scope) = scopes_index_for_brace(scopes, open_brace) else {
            index = open_paren_index + 1;
            continue;
        };

        let end = scopes[body_scope].end;

        for parameter in parse_parameter_names(source, open_paren + 1, close_paren) {
            bindings.push(Binding {
                name: parameter.name,
                start: parameter.start,
                end,
            });
        }

        index = open_paren_index + 1;
    }
}

fn scopes_index_for_brace(scopes: &[Scope], brace_offset: usize) -> Option<usize> {
    scopes
        .iter()
        .enumerate()
        .find_map(|(index, scope)| (scope.start == brace_offset + 1).then_some(index))
}

/// Extrait le premier identifiant et son offset de chaque paramètre top-level.
///
/// Exemples :
/// `func f(a, b)` -> `a`, `b`
/// `func f(a = 10, b = other())` -> `a`, `b`
fn parse_parameter_names(source: &str, start: usize, end: usize) -> Vec<Parameter> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let mut segment_start = start;
    let mut depth = 0usize;

    (start..end).for_each(|index| match bytes[index] {
        b'(' | b'[' | b'{' => depth += 1,
        b')' | b']' | b'}' => depth = depth.saturating_sub(1),
        b',' if depth == 0 => {
            if let Some(parameter) = first_parameter_in_range(source, segment_start, index) {
                result.push(parameter);
            }
            segment_start = index + 1;
        }
        _ => {}
    });

    if let Some(parameter) = first_parameter_in_range(source, segment_start, end) {
        result.push(parameter);
    }

    result
}

fn first_parameter_in_range(source: &str, start: usize, end: usize) -> Option<Parameter> {
    let bytes = source.as_bytes();
    let mut index = start;

    while index < end && bytes[index].is_ascii_whitespace() {
        index += 1;
    }

    if index >= end || !is_identifier_start(bytes[index]) {
        return None;
    }

    let name_start = index;
    index += 1;

    while index < end && is_identifier_byte(bytes[index]) {
        index += 1;
    }

    Some(Parameter {
        name: source[name_start..index].to_string(),
        start: name_start,
    })
}

fn find_matching_char(source: &str, open_offset: usize, open: u8, close: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;

    for index in open_offset..bytes.len() {
        match bytes[index] {
            value if value == open => depth += 1,
            value if value == close => {
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

fn find_next_open_brace(source: &str, start: usize) -> Option<usize> {
    source.as_bytes()[start..]
        .iter()
        .position(|byte| *byte == b'{')
        .map(|relative| start + relative)
}

fn collect_for_variable(
    tokens: &[LexToken],
    for_index: usize,
    brace_scopes: &HashMap<usize, usize>,
    scopes: &mut [Scope],
) {
    let Some(variable_index) = next_identifier_index(tokens, for_index + 1) else {
        return;
    };

    let Some(variable) = token_identifier(tokens, variable_index) else {
        return;
    };

    let Some(in_index) = next_identifier_index(tokens, variable_index + 1) else {
        return;
    };

    if token_identifier(tokens, in_index) != Some("in") {
        return;
    }

    // La variable est visible dans le corps de la boucle et dans ses blocs
    // imbriqués. On la place dans le scope contenant la boucle.
    let scope = innermost_scope_at(scopes, tokens[for_index].start);
    scopes[scope].names.insert(variable.to_string());

    // Si possible, le corps reçoit également le nom pour les sources où la
    // résolution démarre directement à l'intérieur du bloc de boucle.
    let Some(open_brace) = next_punctuation_index(tokens, in_index + 1, b'{') else {
        return;
    };

    if let Some(&body_scope) = brace_scopes.get(&tokens[open_brace].start) {
        scopes[body_scope].names.insert(variable.to_string());
    }
}

fn collect_catch_parameter(
    tokens: &[LexToken],
    catch_index: usize,
    brace_scopes: &HashMap<usize, usize>,
    scopes: &mut [Scope],
) {
    let Some(parameter_index) = next_identifier_index(tokens, catch_index + 1) else {
        return;
    };

    let Some(parameter) = token_identifier(tokens, parameter_index) else {
        return;
    };

    let Some(open_brace) = next_punctuation_index(tokens, parameter_index + 1, b'{') else {
        return;
    };

    if let Some(&body_scope) = brace_scopes.get(&tokens[open_brace].start) {
        scopes[body_scope].names.insert(parameter.to_string());
    }
}

fn next_identifier_index(tokens: &[LexToken], mut index: usize) -> Option<usize> {
    while index < tokens.len() {
        if matches!(&tokens[index].kind, LexTokenKind::Identifier(_)) {
            return Some(index);
        }

        index += 1;
    }

    None
}

fn next_punctuation_index(tokens: &[LexToken], mut index: usize, punctuation: u8) -> Option<usize> {
    while index < tokens.len() {
        if matches!(
            &tokens[index].kind,
            LexTokenKind::Punctuation(value) if *value == punctuation
        ) {
            return Some(index);
        }

        index += 1;
    }

    None
}

fn token_identifier(tokens: &[LexToken], index: usize) -> Option<&str> {
    match &tokens[index].kind {
        LexTokenKind::Identifier(name) => Some(name.as_str()),
        LexTokenKind::Punctuation(_) => None,
    }
}

fn innermost_scope_at(scopes: &[Scope], offset: usize) -> usize {
    scopes
        .iter()
        .enumerate()
        .filter(|(_, scope)| scope.start <= offset && offset < scope.end)
        .max_by_key(|(_, scope)| scope.start)
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn brace_depth_at(source: &str, offset: usize) -> usize {
    source.as_bytes()[..offset]
        .iter()
        .fold(0usize, |depth, byte| match byte {
            b'{' => depth + 1,
            b'}' => depth.saturating_sub(1),
            _ => depth,
        })
}

fn scan_tokens(source: &str) -> Vec<LexToken> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if is_identifier_start(bytes[index]) {
            let start = index;
            index += 1;

            while index < bytes.len() && is_identifier_byte(bytes[index]) {
                index += 1;
            }

            let name = std::str::from_utf8(&bytes[start..index])
                .unwrap_or("")
                .to_string();

            tokens.push(LexToken {
                kind: LexTokenKind::Identifier(name),
                start,
            });

            continue;
        }

        if matches!(
            bytes[index],
            b'(' | b')' | b'{' | b'}' | b'[' | b']' | b',' | b';' | b'.' | b':'
        ) {
            tokens.push(LexToken {
                kind: LexTokenKind::Punctuation(bytes[index]),
                start: index,
            });
        }

        index += 1;
    }

    tokens
}

fn scan_identifiers(source: &str) -> Vec<Ident> {
    let bytes = source.as_bytes();
    let mut ids = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if is_identifier_start(bytes[index]) {
            let start = index;
            index += 1;

            while index < bytes.len() && is_identifier_byte(bytes[index]) {
                index += 1;
            }

            let name = std::str::from_utf8(&bytes[start..index])
                .unwrap_or("")
                .to_string();

            ids.push(Ident {
                name,
                offset: start,
            });
        } else {
            index += 1;
        }
    }

    ids
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

/// Convertit un offset byte en position 1-based (ligne, colonne char).
fn offset_to_position(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut line_start = 0usize;

    for (index, character) in source.char_indices() {
        if index >= offset {
            break;
        }

        if character == '\n' {
            line += 1;
            line_start = index + 1;
        }
    }

    let column = source[line_start..offset].chars().count() + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_with(text: &str) -> Workspace {
        let mut workspace = Workspace::new();
        workspace.open("file:///main.ks".to_string(), 1, text.to_string());
        workspace
    }

    #[test]
    fn ignores_match_wildcard() {
        let source = r#"func daysInMonth(month) {
    match month {
        11 => return 30;
        12 => return 31;
        _ => return 0;
    }
}
"#;

        let workspace = workspace_with(source);
        let diagnostics = analyze(&workspace, "file:///main.ks");

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn detects_undefined_identifier() {
        let source = r#"println("TEST LIST METHODS");

let values = [1, 2, 3];
println(val.size)
"#;

        let workspace = workspace_with(source);
        let diagnostics = analyze(&workspace, "file:///main.ks");

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("val"));
        assert_eq!(diagnostics[0].code.as_deref(), Some("undefined-identifier"));
        assert_eq!(diagnostics[0].severity, 1);
        assert_eq!(diagnostics[0].line, 4);
    }

    #[test]
    fn allows_declared_identifier() {
        let source = r#"let values = [1, 2, 3];
println(values.size)
"#;

        let workspace = workspace_with(source);
        let diagnostics = analyze(&workspace, "file:///main.ks");

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn allows_function_parameter_and_local_variable() {
        let source = r#"class DateTime {}

func addSeconds(value) {
    let result = new DateTime();
    result.value = this.value + value;
    return result;
}
"#;

        let workspace = workspace_with(source);
        let diagnostics = analyze(&workspace, "file:///main.ks");

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn detects_undefined_identifier_inside_function() {
        let source = r#"func addSeconds(value) {
    let result = value + missing;
    return result;
}
"#;

        let workspace = workspace_with(source);
        let diagnostics = analyze(&workspace, "file:///main.ks");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message, "undefined identifier: missing");
        assert_eq!(diagnostics[0].line, 2);
    }

    #[test]
    fn local_variable_is_visible_only_inside_its_block() {
        let source = r#"if true {
    let hidden = 42;
    println(hidden);
}
println(hidden);
"#;

        let workspace = workspace_with(source);
        let diagnostics = analyze(&workspace, "file:///main.ks");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message, "undefined identifier: hidden");
        assert_eq!(diagnostics[0].line, 5);
    }

    #[test]
    fn parameter_is_visible_in_nested_scope() {
        let source = r#"func test(value) {
    if true {
        println(value);
    }
}
"#;

        let workspace = workspace_with(source);
        let diagnostics = analyze(&workspace, "file:///main.ks");

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn detects_typo_between_declared_and_used_identifier() {
        let source = r#"func month() {
    let days = int(this.value / 86400);
    let yer = 1970;

    while days >= this.daysInYear(year) {
        days = days - this.daysInYear(year);
        year = year + 1;
    }

    return days;
}
"#;

        let workspace = workspace_with(source);
        let diagnostics = analyze(&workspace, "file:///main.ks");

        let year_errors = diagnostics
            .iter()
            .filter(|d| d.message == "undefined identifier: year")
            .count();

        assert_eq!(year_errors, 4);
        assert!(
            diagnostics
                .iter()
                .all(|d| d.message != "undefined identifier: yer")
        );
    }

    #[test]
    fn local_names_do_not_leak_between_documents() {
        let mut workspace = Workspace::new();

        workspace.open(
            "file:///first.ks".to_string(),
            1,
            "func first() {\n    let hidden = 42;\n}\n".to_string(),
        );

        workspace.open(
            "file:///second.ks".to_string(),
            1,
            "println(hidden);\n".to_string(),
        );

        let diagnostics = analyze(&workspace, "file:///second.ks");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message, "undefined identifier: hidden");
    }

    #[test]
    fn allows_builtins() {
        let workspace = workspace_with("println(\"hello\")\n");
        let diagnostics = analyze(&workspace, "file:///main.ks");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn ignores_string_contents() {
        let workspace = workspace_with("let x = \"val\"\n");
        let diagnostics = analyze(&workspace, "file:///main.ks");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn ignores_line_comments() {
        let workspace = workspace_with("// val\nlet x = 1\n");
        let diagnostics = analyze(&workspace, "file:///main.ks");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn ignores_block_comments() {
        let workspace = workspace_with("/* val\n   val */\nlet x = 1\n");
        let diagnostics = analyze(&workspace, "file:///main.ks");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn ignores_member_access() {
        let source = "let values = [1, 2, 3];\nprintln(values.size)\n";
        let workspace = workspace_with(source);
        let diagnostics = analyze(&workspace, "file:///main.ks");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn ignores_unknown_uri() {
        let workspace = Workspace::new();
        let diagnostics = analyze(&workspace, "file:///missing.ks");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn detects_undefined_at_correct_column() {
        let workspace = workspace_with("println(val)\n");
        let diagnostics = analyze(&workspace, "file:///main.ks");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 1);
        assert_eq!(diagnostics[0].column, 9);
    }
}
