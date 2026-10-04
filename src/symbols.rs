use std::collections::HashMap;

use kastel::frontend::ast::{GenericParam, Statement, TypeExpr};

use crate::class_index::generic_params_display;
use crate::source_position::span_from_position;
use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Variable,
    Function,
    Class,
    Interface,
    Enum,
    Import,
    TypeAlias,
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
    pub is_exported: bool,
    pub type_display: Option<String>,
    pub signature: Option<String>,
    pub is_mutable: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SymbolIndex {
    symbols: HashMap<String, Symbol>,
}

impl SymbolIndex {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
        }
    }

    pub fn rebuild(&mut self, source: &str, statements: &[Statement]) {
        self.symbols.clear();
        for statement in statements {
            self.collect_statement(source, statement);
        }
    }

    /// Reconstruction tolérante utilisée pendant une frappe si le parser
    /// Kastel n'arrive plus à produire un AST complet.
    pub fn rebuild_fallback(&mut self, source: &str) {
        self.symbols.clear();
        for (line_index, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            let line_no = line_index + 1;
            let base_column = line.len() - trimmed.len() + 1;
            let (exported, body) = if let Some(rest) = trimmed.strip_prefix("export ") {
                (true, rest.trim_start())
            } else {
                (false, trimmed)
            };

            if let Some(rest) = body
                .strip_prefix("let ")
                .or_else(|| body.strip_prefix("const "))
            {
                if let Some(name) = first_identifier(rest) {
                    let mutable = body.starts_with("let ");
                    self.insert(
                        name.to_string(),
                        SymbolKind::Variable,
                        find_name_span(source, line_no, base_column, name),
                        exported,
                        None,
                        None,
                        mutable,
                    );
                }
            } else if let Some(rest) = body.strip_prefix("func ") {
                if let Some(name) = first_identifier(rest) {
                    self.insert(
                        name.to_string(),
                        SymbolKind::Function,
                        find_name_span(source, line_no, base_column, name),
                        exported,
                        None,
                        Some(function_signature_from_line(name, rest)),
                        false,
                    );
                }
            } else if let Some(rest) = body.strip_prefix("class ") {
                if let Some(name) = first_identifier(rest) {
                    self.insert(
                        name.to_string(),
                        SymbolKind::Class,
                        find_name_span(source, line_no, base_column, name),
                        exported,
                        Some(name.to_string()),
                        None,
                        false,
                    );
                }
            } else if let Some(rest) = body.strip_prefix("interface ") {
                if let Some(name) = first_identifier(rest) {
                    self.insert(
                        name.to_string(),
                        SymbolKind::Interface,
                        find_name_span(source, line_no, base_column, name),
                        exported,
                        Some(name.to_string()),
                        None,
                        false,
                    );
                }
            } else if let Some(rest) = body.strip_prefix("enum ") {
                if let Some(name) = first_identifier(rest) {
                    self.insert(
                        name.to_string(),
                        SymbolKind::Enum,
                        find_name_span(source, line_no, base_column, name),
                        exported,
                        Some(name.to_string()),
                        None,
                        false,
                    );
                }
            } else if let Some(rest) = body.strip_prefix("type ") {
                if let Some(name) = first_identifier(rest) {
                    self.insert(
                        name.to_string(),
                        SymbolKind::TypeAlias,
                        find_name_span(source, line_no, base_column, name),
                        exported,
                        None,
                        None,
                        false,
                    );
                }
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.values()
    }
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    fn insert(
        &mut self,
        name: String,
        kind: SymbolKind,
        span: Span,
        is_exported: bool,
        type_display: Option<String>,
        signature: Option<String>,
        is_mutable: bool,
    ) {
        self.symbols.insert(
            name.clone(),
            Symbol {
                name,
                kind,
                span,
                is_exported,
                type_display,
                signature,
                is_mutable,
            },
        );
    }

    fn collect_statement(&mut self, source: &str, statement: &Statement) {
        match statement {
            Statement::Positioned {
                line,
                column,
                statement,
            } => {
                self.collect_positioned(source, statement, *line, *column, false);
            }
            _ => self.collect_positioned(source, statement, 1, 1, false),
        }
    }

    fn collect_positioned(
        &mut self,
        source: &str,
        statement: &Statement,
        line: usize,
        column: usize,
        is_exported: bool,
    ) {
        match statement {
            Statement::Positioned {
                line,
                column,
                statement,
            } => self.collect_positioned(source, statement, *line, *column, is_exported),
            Statement::Export { statement } => {
                self.collect_positioned(source, statement, line, column, true)
            }
            Statement::Let {
                name,
                type_annotation,
                mutable,
                ..
            } => {
                self.insert(
                    name.clone(),
                    SymbolKind::Variable,
                    find_name_span(source, line, column, name),
                    is_exported,
                    type_annotation.as_ref().map(type_expr_display),
                    None,
                    *mutable,
                );
            }
            Statement::Function {
                name,
                generic_params,
                params,
                param_types,
                return_type,
                ..
            } => {
                let signature = function_signature(
                    name,
                    generic_params,
                    params,
                    param_types,
                    return_type,
                );
                self.insert(
                    name.clone(),
                    SymbolKind::Function,
                    find_name_span(source, line, column, name),
                    is_exported,
                    return_type.as_ref().map(type_expr_display),
                    Some(signature),
                    false,
                );
            }
            Statement::Class {
                name,
                generic_params,
                ..
            } => {
                self.insert(
                    name.clone(),
                    SymbolKind::Class,
                    find_name_span(source, line, column, name),
                    is_exported,
                    Some(format!("{}{}", name, generic_params_display(generic_params))),
                    None,
                    false,
                );
            }
            Statement::Interface {
                name,
                generic_params,
                ..
            } => {
                self.insert(
                    name.clone(),
                    SymbolKind::Interface,
                    find_name_span(source, line, column, name),
                    is_exported,
                    Some(format!("{}{}", name, generic_params_display(generic_params))),
                    None,
                    false,
                );
            }
            Statement::Enum {
                name,
                generic_params,
                ..
            } => {
                self.insert(
                    name.clone(),
                    SymbolKind::Enum,
                    find_name_span(source, line, column, name),
                    is_exported,
                    Some(format!("{}{}", name, generic_params_display(generic_params))),
                    None,
                    false,
                );
            }
            Statement::TypeAlias {
                name, type_expr, ..
            } => {
                self.insert(
                    name.clone(),
                    SymbolKind::TypeAlias,
                    find_name_span(source, line, column, name),
                    is_exported,
                    Some(type_expr_display(type_expr)),
                    None,
                    false,
                );
            }
            Statement::Import { path } => {
                if let Some(name) = path.last() {
                    self.insert(
                        name.clone(),
                        SymbolKind::Import,
                        find_name_span(source, line, column, name),
                        is_exported,
                        Some(format!("module {}", path.join("."))),
                        None,
                        false,
                    );
                }
            }
            Statement::FromImport { items, .. } => {
                for item in items {
                    let name = item.alias.as_ref().unwrap_or(&item.name);
                    self.insert(
                        name.clone(),
                        SymbolKind::Import,
                        find_name_span(source, line, column, name),
                        is_exported,
                        None,
                        None,
                        false,
                    );
                }
            }
            Statement::Block(statements) => {
                for statement in statements {
                    self.collect_statement(source, statement);
                }
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                for statement in then_branch {
                    self.collect_statement(source, statement);
                }
                if let Some(statements) = else_branch {
                    for statement in statements {
                        self.collect_statement(source, statement);
                    }
                }
            }
            Statement::While { body, .. } | Statement::ForIn { body, .. } => {
                for statement in body {
                    self.collect_statement(source, statement);
                }
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    for statement in &arm.body {
                        self.collect_statement(source, statement);
                    }
                }
            }
            Statement::Try {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                for statement in try_body {
                    self.collect_statement(source, statement);
                }
                if let Some(statements) = catch_body {
                    for statement in statements {
                        self.collect_statement(source, statement);
                    }
                }
                if let Some(statements) = finally_body {
                    for statement in statements {
                        self.collect_statement(source, statement);
                    }
                }
            }
            _ => {}
        }
    }
}

fn type_expr_display(expr: &TypeExpr) -> String {
    match expr {
        TypeExpr::Named(name) => name.clone(),
        TypeExpr::Generic { name, arguments } => format!(
            "{}<{}>",
            name,
            arguments
                .iter()
                .map(type_expr_display)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeExpr::Union(members) => members
            .iter()
            .map(type_expr_display)
            .collect::<Vec<_>>()
            .join(" | "),
        TypeExpr::Record(fields) => format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(name, ty)| format!("{}: {}", name, type_expr_display(ty)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn function_signature(
    name: &str,
    generic_params: &[GenericParam],
    params: &[String],
    param_types: &[Option<TypeExpr>],
    return_type: &Option<TypeExpr>,
) -> String {
    let args = params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let ty = param_types
                .get(i)
                .and_then(|x| x.as_ref())
                .map(type_expr_display);
            match ty {
                Some(ty) => format!("{}: {}", p, ty),
                None => p.clone(),
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let ret = return_type
        .as_ref()
        .map(type_expr_display)
        .unwrap_or_else(|| "dynamic".to_string());
    format!(
        "func {}{}({}) -> {}",
        name,
        generic_params_display(generic_params),
        args,
        ret
    )
}

fn function_signature_from_line(name: &str, rest: &str) -> String {
    let after_name = rest.strip_prefix(name).unwrap_or(rest).trim_start();
    let header = after_name.split('{').next().unwrap_or(after_name).trim();
    if header.starts_with('(') {
        format!("func {}{}", name, header)
    } else {
        format!("func {}()", name)
    }
}

fn first_identifier(input: &str) -> Option<&str> {
    let start = input.find(|c: char| c.is_ascii_alphabetic() || c == '_')?;
    let bytes = input.as_bytes();
    let mut end = start + 1;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    Some(&input[start..end])
}

fn find_name_span(source: &str, line: usize, column: usize, name: &str) -> Span {
    let line_index = line.saturating_sub(1);
    let line_text = source.lines().nth(line_index);
    let Some(line_text) = line_text else {
        return span_from_position(source, line, column, name.len());
    };
    let search_start = column.saturating_sub(1).min(line_text.len());
    if let Some(relative) = find_identifier(&line_text[search_start..], name) {
        return span_from_position(source, line, search_start + relative + 1, name.len());
    }
    if let Some(relative) = find_identifier(line_text, name) {
        return span_from_position(source, line, relative + 1, name.len());
    }
    span_from_position(source, line, column, name.len())
}

fn find_identifier(source: &str, name: &str) -> Option<usize> {
    let mut offset = 0;
    while offset <= source.len() {
        let relative = source[offset..].find(name)?;
        let start = offset + relative;
        let end = start + name.len();
        if is_identifier_boundary(source, start, end) {
            return Some(start);
        }
        offset = end.max(start + 1);
    }
    None
}

fn is_identifier_boundary(source: &str, start: usize, end: usize) -> bool {
    let before = source[..start].chars().next_back();
    let after = source[end..].chars().next();
    !before.is_some_and(is_identifier_char) && !after.is_some_and(is_identifier_char)
}

fn is_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use kastel::frontend::lexer::lexer::Lexer;
    use kastel::frontend::parser::Parser;

    fn build_index(source: &str) -> SymbolIndex {
        let mut lexer = Lexer::new(source.to_owned());
        let tokens = lexer.scan_token().expect("lexer failed");
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().expect("parser failed");
        let mut index = SymbolIndex::new();
        index.rebuild(source, &statements);
        index
    }

    #[test]
    fn collects_current_declarations() {
        let source = "export const VALUE: int = 42\nfunc hello(a: int) -> str { return \"x\" }\ntype Name = str\n";
        let index = build_index(source);
        assert_eq!(
            index.get("VALUE").and_then(|s| s.type_display.as_deref()),
            Some("int")
        );
        assert!(
            index
                .get("hello")
                .and_then(|s| s.signature.as_deref())
                .unwrap()
                .contains("a: int")
        );
        assert_eq!(
            index.get("Name").map(|s| s.kind),
            Some(SymbolKind::TypeAlias)
        );
    }

    #[test]
    fn fallback_survives_incomplete_source() {
        let source =
            "export func compute(a: int) -> int {\n  let value: List<int> = [1, 2]\n  value.\n";
        let mut index = SymbolIndex::new();
        index.rebuild_fallback(source);
        assert!(index.get("compute").is_some());
    }
}
