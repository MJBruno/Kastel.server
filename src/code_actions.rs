//! Actions de génération de code (`textDocument/codeAction`).
//!
//! Quatre familles d'actions, proposées quand le curseur est dans le corps
//! d'une classe :
//!
//! ```text
//! Generate
//! ├── Getter                  get_<champ>()
//! ├── Setter                  set_<champ>(valeur)
//! ├── Constructor             func initialize(...)
//! └── Interface methods       stubs des méthodes manquantes des interfaces
//! ```
//!
//! Conventions Kastel respectées par le générateur :
//!
//! - le constructeur s'appelle `initialize` (jamais `init`, refusé par le
//!   parser) et peut être surchargé par arité ; ses surcharges doivent toutes
//!   partager la même visibilité, donc une surcharge générée reprend celle
//!   d'un `initialize` déjà présent ;
//! - champs et méthodes partagent le même espace de noms : un getter ne peut
//!   donc pas s'appeler comme son champ, d'où `get_age` / `set_age` ;
//! - les champs `static` ne sont jamais concernés (pas d'instance) ;
//! - `Self` et les paramètres génériques de l'interface sont substitués par
//!   le type de la classe et les arguments de la clause `: Interface<...>` ;
//! - les capabilities intrinsèques (`Add`, `Sub`, `Eq`, `Ord`, ...) sont
//!   reconnues même sans déclaration : `Add<Rhs, Output>` => `add(other: Rhs)
//!   -> Output`, `Eq<Rhs>` => `equals(other: Rhs) -> bool`, `Ord<Rhs>` =>
//!   `compare(other: Rhs) -> int`.

use std::collections::{HashMap, HashSet};

use kastel::frontend::ast::{
    ClassField, FunctionMethod, GenericParam, InterfaceMethod, Statement, TypeExpr, Visibility,
};
use serde_json::{Value, json};

use crate::class_index::{generic_params_display, type_expr_display};
use crate::lsp_position::offset_to_lsp;
use crate::source_position::position_to_offset;
use crate::text_util::{
    is_identifier_boundary, is_identifier_byte, mask_strings_and_comments,
    utf16_character_to_byte_index,
};
use crate::workspace::Workspace;

// ============================================================
// CLASSE CIBLE
// ============================================================

/// Classe du document sous le curseur, avec ses positions dans le texte.
struct ClassTarget<'a> {
    name: &'a str,
    generic_params: &'a [GenericParam],
    bases: &'a [TypeExpr],
    fields: &'a [ClassField],
    methods: &'a [FunctionMethod],
    /// Offset du mot-clé `class`.
    start: usize,
    /// Offset de l'accolade ouvrante du corps.
    open_brace: usize,
    /// Offset de l'accolade fermante du corps.
    close_brace: usize,
}

/// Style d'écriture détecté dans le fichier (fin de ligne, indentation).
struct Style {
    eol: &'static str,
    /// Indentation des membres de la classe.
    member_indent: String,
    /// Une unité d'indentation (`\t` ou quatre espaces).
    unit: String,
}

impl Style {
    fn body_indent(&self) -> String {
        format!("{}{}", self.member_indent, self.unit)
    }
}

/// Descend à travers `Positioned` / `Export` jusqu'à une classe, en gardant
/// la première position rencontrée (toujours antérieure au nom de la classe).
fn find_class_statement(
    statement: &Statement,
    position: Option<(usize, usize)>,
) -> Option<((usize, usize), &Statement)> {
    match statement {
        Statement::Positioned {
            line,
            column,
            statement,
        } => find_class_statement(statement, position.or(Some((*line, *column)))),
        Statement::Export { statement } => find_class_statement(statement, position),
        Statement::Class { .. } => Some((position.unwrap_or((1, 1)), statement)),
        _ => None,
    }
}

/// Localise `class <name> ... { ... }` à partir de `from` dans le texte
/// masqué (chaînes et commentaires neutralisés).
fn locate_class(masked: &str, from: usize, name: &str) -> Option<(usize, usize, usize)> {
    let bytes = masked.as_bytes();
    let mut search_from = from.min(masked.len());

    while let Some(relative) = masked.get(search_from..)?.find("class") {
        let start = search_from + relative;
        let end = start + "class".len();
        search_from = end;

        if !is_identifier_boundary(masked, start, end) {
            continue;
        }

        // Nom de la classe juste après le mot-clé.
        let mut index = end;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let name_start = index;
        while index < bytes.len() && is_identifier_byte(bytes[index]) {
            index += 1;
        }
        if &masked[name_start..index] != name {
            continue;
        }

        let open_brace = masked[index..].find('{').map(|relative| index + relative)?;
        let close_brace = matching_brace(masked, open_brace)?;

        return Some((start, open_brace, close_brace));
    }

    None
}

/// Accolade fermante correspondant à `open` (texte déjà masqué).
fn matching_brace(masked: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;

    for (index, byte) in masked.bytes().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }

    None
}

/// Offset byte d'une position LSP (`line` 0-based, `character` en UTF-16).
fn lsp_to_offset(text: &str, line: u32, character: u32) -> usize {
    let mut offset = 0usize;

    for (current, line_text) in text.split_inclusive('\n').enumerate() {
        if current == line as usize {
            let content = line_text.trim_end_matches(['\r', '\n']);
            return offset + utf16_character_to_byte_index(content, character as usize);
        }

        offset += line_text.len();
    }

    text.len()
}

// ============================================================
// STYLE ET POINTS D'INSERTION
// ============================================================

fn detect_style(text: &str, class: &ClassTarget) -> Style {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };

    let class_line_start = text[..class.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let class_indent: String = text[class_line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();

    let uses_tabs = class_indent.starts_with('\t')
        || (text.contains("\n\t") && !text.contains("\n    "));

    let unit = (if uses_tabs { "\t" } else { "    " }).to_string();

    // Indentation du premier membre déjà présent dans le corps.
    let body = &text[class.open_brace + 1..class.close_brace];
    let member_indent = body
        .split('\n')
        .skip(1) // reste de la ligne de l'accolade ouvrante
        .find(|line| !line.trim().is_empty())
        .map(|line| {
            line.chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect::<String>()
        })
        .filter(|indent| !indent.is_empty())
        .unwrap_or_else(|| format!("{class_indent}{unit}"));

    Style {
        eol,
        member_indent,
        unit,
    }
}

/// `true` si la ligne précédant `offset` n'est ni vide ni une ouverture de
/// bloc : on sépare alors le code inséré par une ligne vide.
fn needs_leading_blank_line(text: &str, offset: usize) -> bool {
    let before = &text[..offset];
    let before = before.strip_suffix('\n').unwrap_or(before);
    let before = before.strip_suffix('\r').unwrap_or(before);
    let last_line = before.rsplit('\n').next().unwrap_or("").trim();

    !(last_line.is_empty() || last_line.ends_with('{'))
}

struct Insertion {
    offset: usize,
    text: String,
}

/// Insère `members` juste avant l'accolade fermante de la classe.
fn insertion_before_close(text: &str, class: &ClassTarget, style: &Style, members: &[String]) -> Insertion {
    let separator = format!("{}{}", style.eol, style.eol);
    let body = members.join(separator.as_str());
    let line_start = text[..class.close_brace]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);

    // Cas courant : `}` seule sur sa ligne.
    if text[line_start..class.close_brace].trim().is_empty() && line_start > class.open_brace {
        let lead = if needs_leading_blank_line(text, line_start) {
            style.eol
        } else {
            ""
        };

        return Insertion {
            offset: line_start,
            text: format!("{lead}{body}{}", style.eol),
        };
    }

    // `class A {}` ou `}` précédée de code : on ouvre une nouvelle ligne.
    let class_line_start = text[..class.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let class_indent: String = text[class_line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();

    Insertion {
        offset: class.close_brace,
        text: format!(
            "{eol}{body}{eol}{class_indent}",
            eol = style.eol,
            body = body,
            class_indent = class_indent
        ),
    }
}

/// Insère `member` juste après la dernière déclaration de champ (la forme
/// habituelle pour un constructeur). Retourne `None` quand la fin du champ ne
/// peut pas être déterminée sans ambiguïté (initialiseur sur plusieurs
/// lignes) : l'appelant se rabat alors sur la fin du corps.
fn insertion_after_last_field(text: &str, class: &ClassTarget, style: &Style, member: &str) -> Option<Insertion> {
    let last = class.fields.iter().max_by_key(|field| (field.line, field.column))?;
    let field_offset = position_to_offset(text, last.line, last.column);

    if field_offset < class.open_brace || field_offset > class.close_brace {
        return None;
    }

    let rest = text.get(field_offset..)?;

    let line_end = rest
        .find('\n')
        .map(|relative| field_offset + relative)
        .unwrap_or(text.len());

    // La déclaration doit tenir sur la ligne : parenthèses, crochets et
    // accolades équilibrés entre le début du champ et la fin de la ligne.
    let masked = mask_strings_and_comments(text.get(field_offset..line_end)?);
    let mut depth = 0i32;
    for byte in masked.bytes() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }

    let after_line = if line_end < text.len() { line_end + 1 } else { line_end };

    let prefix = if after_line == text.len() && !text.ends_with('\n') {
        style.eol
    } else {
        ""
    };

    let next_line = text[after_line..].split('\n').next().unwrap_or("").trim();
    let trailing_blank = !(next_line.is_empty() || next_line.starts_with('}'));

    Some(Insertion {
        offset: after_line,
        text: format!(
            "{prefix}{eol}{member}{eol}{blank}",
            prefix = prefix,
            eol = style.eol,
            member = member,
            blank = if trailing_blank { style.eol } else { "" }
        ),
    })
}

// ============================================================
// GÉNÉRATION : GETTER / SETTER
// ============================================================

/// `_age` => `age` (convention « champ privé préfixé par `_` »).
fn accessor_base(field_name: &str) -> &str {
    let trimmed = field_name.trim_start_matches('_');

    if trimmed.is_empty() { field_name } else { trimmed }
}

fn getter_name(field: &ClassField) -> String {
    format!("get_{}", accessor_base(&field.name))
}

fn setter_name(field: &ClassField) -> String {
    format!("set_{}", accessor_base(&field.name))
}

fn render_getter(field: &ClassField, style: &Style) -> String {
    let indent = &style.member_indent;
    let body_indent = style.body_indent();
    let return_type = field
        .type_annotation
        .as_ref()
        .map(|ty| format!(" -> {}", type_expr_display(ty)))
        .unwrap_or_default();

    format!(
        "{indent}func {name}(){return_type} {{\n{body_indent}return self.{field};\n{indent}}}",
        name = getter_name(field),
        field = field.name,
    )
}

fn render_setter(field: &ClassField, style: &Style) -> String {
    let indent = &style.member_indent;
    let body_indent = style.body_indent();
    let parameter = accessor_base(&field.name);
    let annotation = field
        .type_annotation
        .as_ref()
        .map(|ty| format!(": {}", type_expr_display(ty)))
        .unwrap_or_default();

    format!(
        "{indent}func {name}({parameter}{annotation}) {{\n{body_indent}self.{field} = {parameter};\n{indent}}}",
        name = setter_name(field),
        field = field.name,
    )
}

// ============================================================
// GÉNÉRATION : CONSTRUCTEUR
// ============================================================

fn render_constructor(
    fields: &[&ClassField],
    visibility: Visibility,
    style: &Style,
) -> String {
    let indent = &style.member_indent;
    let body_indent = style.body_indent();

    // Deux champs `_a` et `a` donneraient le même paramètre : on garde alors
    // les noms exacts des champs.
    let bases: Vec<&str> = fields.iter().map(|f| accessor_base(&f.name)).collect();
    let unique: HashSet<&str> = bases.iter().copied().collect();
    let parameters: Vec<&str> = if unique.len() == bases.len() {
        bases
    } else {
        fields.iter().map(|f| f.name.as_str()).collect()
    };

    let signature = fields
        .iter()
        .zip(&parameters)
        .map(|(field, parameter)| match &field.type_annotation {
            Some(ty) => format!("{parameter}: {}", type_expr_display(ty)),
            None => (*parameter).to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");

    let assignments = fields
        .iter()
        .zip(&parameters)
        .map(|(field, parameter)| format!("{body_indent}self.{} = {parameter};", field.name))
        .collect::<Vec<_>>()
        .join("\n");

    let modifier = match visibility {
        Visibility::Public => "",
        Visibility::Protected => "protected ",
        Visibility::Private => "private ",
    };

    format!("{indent}{modifier}func initialize({signature}) {{\n{assignments}\n{indent}}}")
}

/// Visibilité à reprendre pour une surcharge de `initialize` : celle des
/// constructeurs déjà déclarés (le parser exige qu'elle soit identique).
fn constructor_visibility(class: &ClassTarget) -> Visibility {
    class
        .methods
        .iter()
        .find(|method| method.name == kastel::frontend::ast::CONSTRUCTOR_NAME)
        .map(|method| method.visibility)
        .unwrap_or(Visibility::Public)
}

// ============================================================
// GÉNÉRATION : MÉTHODES D'INTERFACE
// ============================================================

#[derive(Debug, Clone)]
struct RequiredMethod {
    name: String,
    /// `<U>` / `<T: Add>` / chaîne vide.
    generics: String,
    /// `(nom, type annoté)`.
    params: Vec<(String, Option<String>)>,
    return_type: Option<String>,
}

struct InterfaceDef {
    generic_params: Vec<String>,
    bases: Vec<TypeExpr>,
    methods: Vec<InterfaceMethod>,
}

fn interface_in_statement(statement: &Statement, name: &str) -> Option<InterfaceDef> {
    match statement {
        Statement::Positioned { statement, .. } | Statement::Export { statement } => {
            interface_in_statement(statement, name)
        }
        Statement::Interface {
            name: declared,
            generic_params,
            bases,
            methods,
        } if declared == name => Some(InterfaceDef {
            generic_params: generic_params.iter().map(|g| g.name.clone()).collect(),
            bases: bases.clone(),
            methods: methods.clone(),
        }),
        _ => None,
    }
}

/// Cherche l'interface `name` : document courant d'abord, puis le reste du
/// workspace (les interfaces importées vivent dans d'autres fichiers).
fn find_interface(workspace: &Workspace, uri: &str, name: &str) -> Option<InterfaceDef> {
    if let Some(document) = workspace.get(uri) {
        for statement in &document.statements {
            if let Some(found) = interface_in_statement(statement, name) {
                return Some(found);
            }
        }
    }

    for (_, document) in workspace.iter() {
        for statement in &document.statements {
            if let Some(found) = interface_in_statement(statement, name) {
                return Some(found);
            }
        }
    }

    None
}

fn substitute(
    expr: &TypeExpr,
    map: &HashMap<String, TypeExpr>,
    self_type: &TypeExpr,
) -> TypeExpr {
    match expr {
        TypeExpr::Named(name) => {
            if name == "Self" {
                self_type.clone()
            } else if let Some(replacement) = map.get(name) {
                replacement.clone()
            } else {
                expr.clone()
            }
        }
        TypeExpr::Generic { name, arguments } => TypeExpr::Generic {
            name: name.clone(),
            arguments: arguments
                .iter()
                .map(|argument| substitute(argument, map, self_type))
                .collect(),
        },
        TypeExpr::Union(members) => TypeExpr::Union(
            members
                .iter()
                .map(|member| substitute(member, map, self_type))
                .collect(),
        ),
        TypeExpr::Record(fields) => TypeExpr::Record(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), substitute(ty, map, self_type)))
                .collect(),
        ),
        TypeExpr::Tuple(items) => TypeExpr::Tuple(
            items
                .iter()
                .map(|item| substitute(item, map, self_type))
                .collect(),
        ),
        TypeExpr::Function {
            params,
            return_type,
        } => TypeExpr::Function {
            params: params
                .iter()
                .map(|param| substitute(param, map, self_type))
                .collect(),
            return_type: Box::new(substitute(return_type, map, self_type)),
        },
    }
}

fn render_required(
    method: &InterfaceMethod,
    map: &HashMap<String, TypeExpr>,
    self_type: &TypeExpr,
) -> RequiredMethod {
    RequiredMethod {
        name: method.name.clone(),
        generics: generic_params_display(&method.generic_params),
        params: method
            .params
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let annotation = method
                    .param_types
                    .get(index)
                    .and_then(Option::as_ref)
                    .map(|ty| type_expr_display(&substitute(ty, map, self_type)));
                (name.clone(), annotation)
            })
            .collect(),
        return_type: method
            .return_type
            .as_ref()
            .map(|ty| type_expr_display(&substitute(ty, map, self_type))),
    }
}

/// Méthode exigée par une capability intrinsèque (`Add`, `Eq`, `Ord`, ...).
///
/// Alignée sur `Capability::operator_method_name` / `generic_arity` du
/// compilateur. `Index`/`IndexMut` ne sont volontairement pas gérées : ce
/// compilateur ne les branche pas.
fn builtin_capability_method(
    name: &str,
    arguments: &[TypeExpr],
    self_type: &TypeExpr,
) -> Option<RequiredMethod> {
    let (method, arity, fixed_output) = match name {
        "Add" => ("add", 2, None),
        "Sub" => ("sub", 2, None),
        "Mul" => ("mul", 2, None),
        "Div" => ("div", 2, None),
        "Mod" => ("mod", 2, None),
        "BitAnd" => ("bitand", 2, None),
        "BitOr" => ("bitor", 2, None),
        "BitXor" => ("bitxor", 2, None),
        "ShiftLeft" => ("shl", 2, None),
        "ShiftRight" => ("shr", 2, None),
        "Eq" => ("equals", 1, Some("bool")),
        "Ord" => ("compare", 1, Some("int")),
        _ => return None,
    };

    // `Add` seul = `Add<Self, Self>` ; `Eq` seul = `Eq<Self>`.
    let (rhs, output) = if arguments.is_empty() {
        (self_type.clone(), self_type.clone())
    } else if arguments.len() == arity {
        (
            arguments[0].clone(),
            arguments.get(1).cloned().unwrap_or_else(|| self_type.clone()),
        )
    } else {
        return None;
    };

    let return_type = match fixed_output {
        Some(fixed) => fixed.to_string(),
        None => type_expr_display(&output),
    };

    Some(RequiredMethod {
        name: method.to_string(),
        generics: String::new(),
        params: vec![("other".to_string(), Some(type_expr_display(&rhs)))],
        return_type: Some(return_type),
    })
}

/// Rassemble les méthodes exigées par `base` (et par ses interfaces parentes).
fn collect_required(
    workspace: &Workspace,
    uri: &str,
    base: &TypeExpr,
    self_type: &TypeExpr,
    visited: &mut HashSet<String>,
    out: &mut Vec<RequiredMethod>,
) {
    let (name, arguments): (String, Vec<TypeExpr>) = match base {
        TypeExpr::Named(name) => (name.clone(), Vec::new()),
        TypeExpr::Generic { name, arguments } => (name.clone(), arguments.clone()),
        _ => return,
    };

    if !visited.insert(name.clone()) {
        return;
    }

    if let Some(definition) = find_interface(workspace, uri, &name) {
        let mut map = HashMap::new();
        for (parameter, argument) in definition.generic_params.iter().zip(arguments.iter()) {
            map.insert(parameter.clone(), argument.clone());
        }

        for method in &definition.methods {
            out.push(render_required(method, &map, self_type));
        }

        for parent in &definition.bases {
            let parent = substitute(parent, &map, self_type);
            collect_required(workspace, uri, &parent, self_type, visited, out);
        }

        return;
    }

    if let Some(method) = builtin_capability_method(&name, &arguments, self_type) {
        out.push(method);
    }
}

fn class_self_type(class: &ClassTarget) -> TypeExpr {
    if class.generic_params.is_empty() {
        TypeExpr::Named(class.name.to_string())
    } else {
        TypeExpr::Generic {
            name: class.name.to_string(),
            arguments: class
                .generic_params
                .iter()
                .map(|param| TypeExpr::Named(param.name.clone()))
                .collect(),
        }
    }
}

/// Méthodes d'interface pas encore implémentées (même nom et même arité
/// qu'une méthode d'instance de la classe), dédoublonnées entre interfaces.
fn missing_interface_methods(
    workspace: &Workspace,
    uri: &str,
    class: &ClassTarget,
) -> Vec<(String, RequiredMethod)> {
    let self_type = class_self_type(class);
    let mut visited = HashSet::new();
    let mut required: Vec<(String, RequiredMethod)> = Vec::new();

    for base in class.bases {
        let mut collected = Vec::new();
        collect_required(workspace, uri, base, &self_type, &mut visited, &mut collected);

        let interface_name = match base {
            TypeExpr::Named(name) => name.clone(),
            TypeExpr::Generic { name, .. } => name.clone(),
            other => type_expr_display(other),
        };

        for method in collected {
            required.push((interface_name.clone(), method));
        }
    }

    let mut seen: HashSet<(String, usize)> = HashSet::new();

    required
        .into_iter()
        .filter(|(_, method)| {
            let implemented = class.methods.iter().any(|existing| {
                !existing.is_static
                    && existing.name == method.name
                    && existing.params.len() == method.params.len()
            });

            !implemented && seen.insert((method.name.clone(), method.params.len()))
        })
        .collect()
}

fn render_stub(method: &RequiredMethod, class_name: &str, style: &Style) -> String {
    let indent = &style.member_indent;
    let body_indent = style.body_indent();

    let parameters = method
        .params
        .iter()
        .map(|(name, annotation)| match annotation {
            Some(ty) => format!("{name}: {ty}"),
            None => name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");

    let returns_value = match method.return_type.as_deref() {
        None | Some("None") => false,
        Some(_) => true,
    };

    let return_type = method
        .return_type
        .as_ref()
        .map(|ty| format!(" -> {ty}"))
        .unwrap_or_default();

    let body = if returns_value {
        format!(
            "{body_indent}throw \"not implemented: {class_name}.{name}\";",
            name = method.name
        )
    } else {
        format!(
            "{body_indent}// TODO: implémenter {class_name}.{name}",
            name = method.name
        )
    };

    format!(
        "{indent}func {name}{generics}({parameters}){return_type} {{\n{body}\n{indent}}}",
        name = method.name,
        generics = method.generics,
    )
}

// ============================================================
// POINT D'ENTRÉE
// ============================================================

/// `true` si `kind` est accepté par le filtre `context.only` du client.
fn kind_allowed(kind: &str, only: &[String]) -> bool {
    only.is_empty()
        || only.iter().any(|requested| {
            kind == requested
                || kind.starts_with(&format!("{requested}."))
                || requested.starts_with(&format!("{kind}."))
        })
}

/// Base des kinds : `source.generate` quand le client ne demande que des
/// actions « source » (menu *Source Action…* de VS Code), sinon
/// `refactor.generate` (visible dans l'ampoule / `Ctrl+.`).
fn kind_base(only: &[String]) -> &'static str {
    if !only.is_empty() && only.iter().all(|kind| kind.starts_with("source")) {
        "source.generate"
    } else {
        "refactor.generate"
    }
}

fn make_action(uri: &str, text: &str, title: String, kind: String, insertions: Vec<Insertion>) -> Value {
    let edits: Vec<Value> = insertions
        .into_iter()
        .map(|insertion| {
            let (line, character) = offset_to_lsp(text, insertion.offset);
            json!({
                "range": {
                    "start": { "line": line, "character": character },
                    "end": { "line": line, "character": character },
                },
                "newText": insertion.text,
            })
        })
        .collect();

    json!({
        "title": title,
        "kind": kind,
        "edit": { "changes": { uri: edits } },
    })
}

/// Convertit les fins de ligne `\n` d'un bloc généré vers celles du fichier.
fn with_eol(block: String, style: &Style) -> String {
    if style.eol == "\n" {
        block
    } else {
        block.replace('\n', style.eol)
    }
}

/// Construit la liste des actions de génération pour la position
/// `(line, character)` (LSP : ligne 0-based, `character` en UTF-16).
pub fn build_code_actions(
    workspace: &Workspace,
    uri: &str,
    line: u32,
    character: u32,
    only: &[String],
) -> Value {
    let Some(document) = workspace.get(uri) else {
        return Value::Array(Vec::new());
    };

    let text = document.text.as_str();
    let masked = mask_strings_and_comments(text);
    let cursor = lsp_to_offset(text, line, character);

    // Classe contenant le curseur.
    let mut target: Option<ClassTarget> = None;

    for statement in &document.statements {
        let Some(((class_line, class_column), class_statement)) =
            find_class_statement(statement, None)
        else {
            continue;
        };

        let Statement::Class {
            name,
            generic_params,
            bases,
            fields,
            methods,
        } = class_statement
        else {
            continue;
        };

        let from = position_to_offset(text, class_line, class_column);
        let Some((start, open_brace, close_brace)) = locate_class(&masked, from, name) else {
            continue;
        };

        if cursor >= start && cursor <= close_brace {
            target = Some(ClassTarget {
                name,
                generic_params,
                bases,
                fields,
                methods,
                start,
                open_brace,
                close_brace,
            });
            break;
        }
    }

    let Some(class) = target else {
        return Value::Array(Vec::new());
    };

    let style = detect_style(text, &class);
    let base_kind = kind_base(only);
    let mut actions: Vec<Value> = Vec::new();

    let mut push = |suffix: &str, title: String, insertions: Vec<Insertion>| {
        let kind = format!("{base_kind}.{suffix}");
        if kind_allowed(&kind, only) {
            actions.push(make_action(uri, text, title, kind, insertions));
        }
    };

    let existing_names: HashSet<&str> = class
        .methods
        .iter()
        .map(|method| method.name.as_str())
        .chain(class.fields.iter().map(|field| field.name.as_str()))
        .collect();

    let instance_fields: Vec<&ClassField> =
        class.fields.iter().filter(|field| !field.is_static).collect();

    let focused_field: Option<&ClassField> = instance_fields
        .iter()
        .copied()
        .find(|field| field.line == line as usize + 1);

    // --------------------------------------------------------
    // GETTERS / SETTERS
    // --------------------------------------------------------

    let missing_getters: Vec<&ClassField> = instance_fields
        .iter()
        .copied()
        .filter(|field| !existing_names.contains(getter_name(field).as_str()))
        .collect();

    let missing_setters: Vec<&ClassField> = instance_fields
        .iter()
        .copied()
        .filter(|field| !existing_names.contains(setter_name(field).as_str()))
        .collect();

    let before_close = |members: Vec<String>| -> Vec<Insertion> {
        let members: Vec<String> = members.into_iter().map(|m| with_eol(m, &style)).collect();
        vec![insertion_before_close(text, &class, &style, &members)]
    };

    // Actions ciblées sur le champ sous le curseur.
    if let Some(field) = focused_field {
        let getter_missing = missing_getters.iter().any(|f| f.name == field.name);
        let setter_missing = missing_setters.iter().any(|f| f.name == field.name);

        if getter_missing {
            push(
                "getter",
                format!("Générer le getter {}()", getter_name(field)),
                before_close(vec![render_getter(field, &style)]),
            );
        }

        if setter_missing {
            push(
                "setter",
                format!("Générer le setter {}()", setter_name(field)),
                before_close(vec![render_setter(field, &style)]),
            );
        }

        if getter_missing && setter_missing {
            push(
                "accessors",
                format!("Générer getter et setter pour '{}'", field.name),
                before_close(vec![
                    render_getter(field, &style),
                    render_setter(field, &style),
                ]),
            );
        }
    }

    // Actions pour toute la classe.
    if !missing_getters.is_empty() {
        let members = missing_getters
            .iter()
            .map(|field| render_getter(field, &style))
            .collect();
        push(
            "getter",
            format!("Générer les getters ({})", missing_getters.len()),
            before_close(members),
        );
    }

    if !missing_setters.is_empty() {
        let members = missing_setters
            .iter()
            .map(|field| render_setter(field, &style))
            .collect();
        push(
            "setter",
            format!("Générer les setters ({})", missing_setters.len()),
            before_close(members),
        );
    }

    if !missing_getters.is_empty() && !missing_setters.is_empty() {
        let mut members = Vec::new();
        for field in &instance_fields {
            if missing_getters.iter().any(|f| f.name == field.name) {
                members.push(render_getter(field, &style));
            }
            if missing_setters.iter().any(|f| f.name == field.name) {
                members.push(render_setter(field, &style));
            }
        }
        push(
            "accessors",
            "Générer getters et setters".to_string(),
            before_close(members),
        );
    }

    // --------------------------------------------------------
    // CONSTRUCTEUR
    // --------------------------------------------------------

    let visibility = constructor_visibility(&class);
    let arity_taken = |arity: usize| {
        class
            .methods
            .iter()
            .any(|method| method.name == kastel::frontend::ast::CONSTRUCTOR_NAME && method.params.len() == arity)
    };

    let without_defaults: Vec<&ClassField> = instance_fields
        .iter()
        .copied()
        .filter(|field| field.initializer.is_none())
        .collect();

    let mut constructor_variants: Vec<(String, Vec<&ClassField>)> = Vec::new();

    if !without_defaults.is_empty() && without_defaults.len() != instance_fields.len() {
        constructor_variants.push((
            "Générer le constructeur (champs sans valeur par défaut)".to_string(),
            without_defaults,
        ));
        constructor_variants.push((
            "Générer le constructeur (tous les champs)".to_string(),
            instance_fields.clone(),
        ));
    } else if !instance_fields.is_empty() {
        constructor_variants.push((
            "Générer le constructeur initialize(...)".to_string(),
            instance_fields.clone(),
        ));
    }

    for (title, fields) in constructor_variants {
        // Une surcharge de même arité serait ambiguë : on ne la propose pas.
        if arity_taken(fields.len()) {
            continue;
        }

        let constructor = with_eol(render_constructor(&fields, visibility, &style), &style);

        let insertion = insertion_after_last_field(text, &class, &style, &constructor)
            .unwrap_or_else(|| insertion_before_close(text, &class, &style, &[constructor.clone()]));

        push("constructor", title, vec![insertion]);
    }

    // --------------------------------------------------------
    // MÉTHODES D'INTERFACE
    // --------------------------------------------------------

    let missing = missing_interface_methods(workspace, uri, &class);

    if !missing.is_empty() {
        let mut interfaces: Vec<&str> = Vec::new();
        for (interface, _) in &missing {
            if !interfaces.contains(&interface.as_str()) {
                interfaces.push(interface.as_str());
            }
        }

        let members: Vec<String> = missing
            .iter()
            .map(|(_, method)| with_eol(render_stub(method, class.name, &style), &style))
            .collect();

        push(
            "interface",
            format!(
                "Implémenter les méthodes manquantes ({}) : {}",
                missing.len(),
                interfaces.join(", ")
            ),
            vec![insertion_before_close(text, &class, &style, &members)],
        );
    }

    Value::Array(actions)
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    const URI: &str = "file:///main.ks";

    fn workspace_with(text: &str) -> Workspace {
        let mut workspace = Workspace::new();
        workspace.open(URI.to_string(), 1, text.to_string());
        workspace
    }

    fn actions_at(text: &str, line: u32, only: &[&str]) -> Vec<Value> {
        let workspace = workspace_with(text);
        let only: Vec<String> = only.iter().map(|s| s.to_string()).collect();
        match build_code_actions(&workspace, URI, line, 0, &only) {
            Value::Array(items) => items,
            other => panic!("tableau attendu, reçu {other:?}"),
        }
    }

    fn titles(actions: &[Value]) -> Vec<String> {
        actions
            .iter()
            .map(|a| a["title"].as_str().unwrap_or("").to_string())
            .collect()
    }

    fn find<'a>(actions: &'a [Value], fragment: &str) -> &'a Value {
        actions
            .iter()
            .find(|a| a["title"].as_str().is_some_and(|t| t.contains(fragment)))
            .unwrap_or_else(|| panic!("action '{fragment}' absente : {:?}", titles(actions)))
    }

    fn new_text(action: &Value) -> String {
        action["edit"]["changes"][URI][0]["newText"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    const PERSON: &str = r#"class Person {
    private let name: str;
    private let age: int = 0;

    func greet() -> str {
        return self.name;
    }
}
"#;

    #[test]
    fn no_actions_outside_a_class() {
        let source = format!("let x = 1;\n{PERSON}");
        assert!(actions_at(&source, 0, &[]).is_empty());
    }

    #[test]
    fn generates_getters_for_every_instance_field() {
        let actions = actions_at(PERSON, 4, &[]);
        let action = find(&actions, "Générer les getters (2)");
        let text = new_text(action);

        assert!(text.contains("func get_name() -> str {"));
        assert!(text.contains("return self.name;"));
        assert!(text.contains("func get_age() -> int {"));
        assert!(text.contains("return self.age;"));
    }

    #[test]
    fn generates_setters_with_typed_parameter() {
        let actions = actions_at(PERSON, 4, &[]);
        let text = new_text(find(&actions, "Générer les setters (2)"));

        assert!(text.contains("func set_age(age: int) {"));
        assert!(text.contains("self.age = age;"));
    }

    #[test]
    fn existing_accessor_is_not_regenerated() {
        let source = r#"class Person {
    private let name: str;
    func get_name() -> str {
        return self.name;
    }
}
"#;
        let actions = actions_at(source, 2, &[]);

        assert!(
            actions
                .iter()
                .all(|a| !a["title"].as_str().unwrap_or("").contains("getters")),
            "{:?}",
            titles(&actions)
        );
        assert!(
            titles(&actions)
                .iter()
                .any(|t| t.contains("Générer les setters (1)"))
        );
    }

    #[test]
    fn field_under_cursor_gets_targeted_actions() {
        // Ligne 2 (0-based) = `private let age: int = 0;`
        let actions = actions_at(PERSON, 2, &[]);
        let all = titles(&actions);

        assert!(all.iter().any(|t| t.contains("get_age")), "{all:?}");
        assert!(all.iter().any(|t| t.contains("set_age")), "{all:?}");
        assert!(
            all.iter().any(|t| t.contains("getter et setter pour 'age'")),
            "{all:?}"
        );
    }

    #[test]
    fn leading_underscore_is_stripped_from_accessor_names() {
        let source = "class A {\n    private let _count: int;\n}\n";
        let actions = actions_at(source, 1, &[]);
        let text = new_text(find(&actions, "Générer les getters"));

        assert!(text.contains("func get_count() -> int {"));
        assert!(text.contains("return self._count;"));
    }

    #[test]
    fn constructor_uses_initialize_and_skips_static_fields() {
        let source = r#"class Point {
    static let created: int = 0;
    let x: int;
    let y: int;
}
"#;
        let actions = actions_at(source, 2, &[]);
        let text = new_text(find(&actions, "constructeur"));

        assert!(text.contains("func initialize(x: int, y: int) {"), "{text}");
        assert!(text.contains("self.x = x;"));
        assert!(text.contains("self.y = y;"));
        assert!(!text.contains("created"));
    }

    #[test]
    fn constructor_is_inserted_after_the_last_field() {
        let source = "class P {\n    let a: int;\n    let b: int;\n\n    func f() { }\n}\n";
        let actions = actions_at(source, 1, &[]);
        let action = find(&actions, "constructeur");
        let edit = &action["edit"]["changes"][URI][0];

        // Après la ligne 2 (dernier champ) => insertion à la ligne 3.
        assert_eq!(edit["range"]["start"]["line"], 3);
    }

    #[test]
    fn constructor_offers_both_variants_when_some_fields_have_defaults() {
        let actions = actions_at(PERSON, 4, &[]);
        let all = titles(&actions);

        assert!(
            all.iter().any(|t| t.contains("champs sans valeur par défaut")),
            "{all:?}"
        );
        assert!(all.iter().any(|t| t.contains("tous les champs")), "{all:?}");

        let narrow = new_text(find(&actions, "sans valeur par défaut"));
        assert!(narrow.contains("func initialize(name: str) {"), "{narrow}");

        let wide = new_text(find(&actions, "tous les champs"));
        assert!(wide.contains("func initialize(name: str, age: int) {"), "{wide}");
    }

    #[test]
    fn constructor_overload_keeps_existing_visibility() {
        let source = r#"class P {
    let a: int;
    let b: int;
    private func initialize(a: int) {
        self.a = a;
    }
}
"#;
        let actions = actions_at(source, 1, &[]);
        let text = new_text(find(&actions, "constructeur"));

        assert!(
            text.contains("private func initialize(a: int, b: int) {"),
            "{text}"
        );
    }

    #[test]
    fn constructor_with_same_arity_is_not_offered() {
        let source = r#"class P {
    let a: int;
    func initialize(a: int) {
        self.a = a;
    }
}
"#;
        let actions = actions_at(source, 1, &[]);

        assert!(
            titles(&actions).iter().all(|t| !t.contains("constructeur")),
            "{:?}",
            titles(&actions)
        );
    }

    #[test]
    fn generates_stubs_for_user_interface_methods() {
        let source = r#"interface Shape {
    func area() -> float;
    func scale(factor: float) -> Self;
    func describe() -> None;
}

class Square : Shape {
    let side: float;
}
"#;
        let actions = actions_at(source, 7, &[]);
        let text = new_text(find(&actions, "Implémenter les méthodes manquantes"));

        assert!(text.contains("func area() -> float {"), "{text}");
        assert!(text.contains("throw \"not implemented: Square.area\";"), "{text}");
        // `Self` est remplacé par le nom de la classe.
        assert!(text.contains("func scale(factor: float) -> Square {"), "{text}");
        // Retour None : simple commentaire, pas de `throw`.
        assert!(text.contains("func describe() -> None {"), "{text}");
        assert!(text.contains("// TODO: implémenter Square.describe"), "{text}");
    }

    #[test]
    fn already_implemented_interface_method_is_skipped() {
        let source = r#"interface Shape {
    func area() -> float;
    func name() -> str;
}

class Square : Shape {
    func area() -> float {
        return 1.0;
    }
}
"#;
        let actions = actions_at(source, 6, &[]);
        let text = new_text(find(&actions, "Implémenter les méthodes manquantes (1)"));

        assert!(text.contains("func name() -> str {"));
        assert!(!text.contains("func area"));
    }

    #[test]
    fn generic_interface_arguments_are_substituted() {
        let source = r#"interface Container<T> {
    func put(item: T);
    func take() -> Option<T>;
}

class Box : Container<int> {
}
"#;
        let actions = actions_at(source, 5, &[]);
        let text = new_text(find(&actions, "Implémenter les méthodes manquantes"));

        assert!(text.contains("func put(item: int) {"), "{text}");
        assert!(text.contains("func take() -> Option<int> {"), "{text}");
    }

    #[test]
    fn builtin_capabilities_are_recognised_without_declaration() {
        let source = r#"class Money : Add, Eq<Money>, Ord<Money> {
    let cents: int;
}
"#;
        let actions = actions_at(source, 1, &[]);
        let text = new_text(find(&actions, "Implémenter les méthodes manquantes (3)"));

        assert!(text.contains("func add(other: Money) -> Money {"), "{text}");
        assert!(text.contains("func equals(other: Money) -> bool {"), "{text}");
        assert!(text.contains("func compare(other: Money) -> int {"), "{text}");
    }

    #[test]
    fn heterogeneous_capability_uses_rhs_and_output() {
        let source = "class Vec2 : Mul<float, Vec2> {\n    let x: float;\n}\n";
        let actions = actions_at(source, 1, &[]);
        let text = new_text(find(&actions, "Implémenter les méthodes manquantes"));

        assert!(text.contains("func mul(other: float) -> Vec2 {"), "{text}");
    }

    #[test]
    fn interface_inherited_through_another_interface_is_collected() {
        let source = r#"interface Named {
    func name() -> str;
}

interface Greeter : Named {
    func greet() -> str;
}

class Person : Greeter {
}
"#;
        let actions = actions_at(source, 8, &[]);
        let text = new_text(find(&actions, "Implémenter les méthodes manquantes (2)"));

        assert!(text.contains("func greet() -> str {"));
        assert!(text.contains("func name() -> str {"));
    }

    #[test]
    fn only_filter_restricts_action_kinds() {
        let source = "class A {\n    let x: int;\n}\n";

        assert!(actions_at(source, 1, &["quickfix"]).is_empty());

        let source_actions = actions_at(source, 1, &["source"]);
        assert!(!source_actions.is_empty());
        assert!(
            source_actions
                .iter()
                .all(|a| a["kind"].as_str().unwrap_or("").starts_with("source.generate"))
        );

        let refactor_actions = actions_at(source, 1, &["refactor"]);
        assert!(
            refactor_actions
                .iter()
                .all(|a| a["kind"].as_str().unwrap_or("").starts_with("refactor.generate"))
        );
    }

    #[test]
    fn empty_class_on_one_line_still_gets_a_valid_edit() {
        let source = "class A : Eq<A> {}\n";
        let actions = actions_at(source, 0, &[]);
        let action = find(&actions, "Implémenter les méthodes manquantes");
        let text = new_text(action);

        assert!(text.starts_with('\n'), "{text:?}");
        assert!(text.contains("func equals(other: A) -> bool {"));
    }

    #[test]
    fn crlf_files_keep_their_line_endings() {
        let source = "class A {\r\n    let x: int;\r\n}\r\n";
        let actions = actions_at(source, 1, &[]);
        let text = new_text(find(&actions, "Générer les getters"));

        assert!(text.contains("\r\n"));
        assert!(!text.replace("\r\n", "").contains('\n'), "{text:?}");
    }
}
