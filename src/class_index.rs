//! Index des classes/interfaces de Kastel pour l'IntelliSense.
//!
//! Contrairement à l'ancienne implémentation, l'index lit les champs déclarés
//! (`let field: Type`) et leurs visibilités directement depuis l'AST. Il ne
//! déduit donc plus un champ uniquement après avoir rencontré `this.field =`.

use std::collections::HashMap;

use kastel::compiler::types::{FunctionType, Type};
use kastel::frontend::ast::{AssignmentTarget, Expression, Statement, TypeExpr, Visibility};

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct FieldInfo {
    pub name: String,
    pub type_annotation: Option<TypeExpr>,
    pub visibility: Visibility,
}

#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: String,
    pub params: Vec<String>,
    pub param_types: Vec<Option<TypeExpr>>,
    pub return_type: Option<TypeExpr>,
    pub visibility: Visibility,
}

impl MethodInfo {
    pub fn signature(&self) -> String {
        let params = self
            .params
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let ty = self
                    .param_types
                    .get(index)
                    .and_then(Option::as_ref)
                    .map(type_expr_display)
                    .unwrap_or_else(|| "dynamic".to_string());

                format!("{}: {}", name, ty)
            })
            .collect::<Vec<_>>()
            .join(", ");

        let return_type = self
            .return_type
            .as_ref()
            .map(type_expr_display)
            .map(|ty| format!(" -> {}", ty))
            .unwrap_or_default();

        format!("func {}({}){}", self.name, params, return_type)
    }

    pub fn function_type(&self) -> FunctionType {
        FunctionType {
            params: self
                .param_types
                .iter()
                .map(|annotation| {
                    annotation
                        .as_ref()
                        .map(|expr| Type::from_type_expr(expr))
                        .unwrap_or(Type::Dynamic)
                })
                .collect(),
            return_type: Box::new(
                self.return_type
                    .as_ref()
                    .map(Type::from_type_expr)
                    .unwrap_or(Type::Dynamic),
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
#[allow(unused)]
pub struct ClassInfo {
    pub bases: Vec<String>,
    pub methods: Vec<MethodInfo>,
    pub fields: Vec<String>,
    pub field_info: HashMap<String, FieldInfo>,
    pub is_interface: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ClassIndex {
    classes: HashMap<String, ClassInfo>,
}

impl ClassIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rebuild(&mut self, statements: &[Statement]) {
        self.classes.clear();

        for statement in statements {
            self.collect_statement(statement);
        }
    }

    pub fn get(&self, name: &str) -> Option<&ClassInfo> {
        self.classes.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.classes.contains_key(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.classes.keys()
    }

    pub fn all_methods(&self, name: &str) -> Vec<&MethodInfo> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        self.collect_methods(name, &mut out, &mut seen);
        out
    }

    pub fn method_owner(&self, name: &str, method_name: &str) -> Option<String> {
        let mut seen = std::collections::HashSet::new();
        self.find_method_owner(name, method_name, &mut seen)
    }

    pub fn all_fields(&self, name: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        self.collect_fields(name, &mut out, &mut seen);
        out
    }

    pub fn field(&self, class_name: &str, field_name: &str) -> Option<&FieldInfo> {
        let mut seen = std::collections::HashSet::new();
        self.find_field(class_name, field_name, &mut seen)
    }

    pub fn field_owner(&self, class_name: &str, field_name: &str) -> Option<String> {
        let mut seen = std::collections::HashSet::new();
        self.find_field_owner(class_name, field_name, &mut seen)
    }

    pub fn method(&self, class_name: &str, method_name: &str) -> Option<&MethodInfo> {
        let mut seen = std::collections::HashSet::new();
        self.find_method(class_name, method_name, &mut seen)
    }

    // pub fn method_function_type(&self, class_name: &str, method_name: &str) -> Option<Type> {
    //     self.method(class_name, method_name)
    //         .map(|method| Type::Function(method.function_type()))
    // }

    fn find_field_owner(
        &self,
        name: &str,
        field_name: &str,
        seen: &mut std::collections::HashSet<String>,
    ) -> Option<String> {
        if !seen.insert(name.to_string()) {
            return None;
        }

        let info = self.classes.get(name)?;
        if info.fields.iter().any(|field| field == field_name) {
            return Some(name.to_string());
        }

        for base in &info.bases {
            if let Some(owner) = self.find_field_owner(base, field_name, seen) {
                return Some(owner);
            }
        }

        None
    }

    fn find_method_owner(
        &self,
        name: &str,
        method_name: &str,
        seen: &mut std::collections::HashSet<String>,
    ) -> Option<String> {
        if !seen.insert(name.to_string()) {
            return None;
        }

        let info = self.classes.get(name)?;

        if info.methods.iter().any(|method| method.name == method_name) {
            return Some(name.to_string());
        }

        for base in &info.bases {
            if let Some(owner) = self.find_method_owner(base, method_name, seen) {
                return Some(owner);
            }
        }

        None
    }

    fn find_method(
        &self,
        name: &str,
        method_name: &str,
        seen: &mut std::collections::HashSet<String>,
    ) -> Option<&MethodInfo> {
        if !seen.insert(name.to_string()) {
            return None;
        }

        let info = self.classes.get(name)?;
        if let Some(method) = info
            .methods
            .iter()
            .find(|method| method.name == method_name)
        {
            return Some(method);
        }

        for base in &info.bases {
            if let Some(method) = self.find_method(base, method_name, seen) {
                return Some(method);
            }
        }

        None
    }

    fn find_field(
        &self,
        name: &str,
        field_name: &str,
        seen: &mut std::collections::HashSet<String>,
    ) -> Option<&FieldInfo> {
        if !seen.insert(name.to_string()) {
            return None;
        }

        let info = self.classes.get(name)?;
        if let Some(field) = info.field_info.get(field_name) {
            return Some(field);
        }

        for base in &info.bases {
            if let Some(field) = self.find_field(base, field_name, seen) {
                return Some(field);
            }
        }

        None
    }

    fn collect_methods<'a>(
        &'a self,
        name: &str,
        out: &mut Vec<&'a MethodInfo>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        if !seen.insert(name.to_string()) {
            return;
        }

        let Some(info) = self.classes.get(name) else {
            return;
        };

        for method in &info.methods {
            // Méthode générée par le désucrage interne des initialiseurs de
            // champs : elle ne fait pas partie de l'API visible.
            if !method.name.starts_with("__fields_") {
                out.push(method);
            }
        }

        for base in &info.bases {
            self.collect_methods(base, out, seen);
        }
    }

    fn collect_fields<'a>(
        &'a self,
        name: &str,
        out: &mut Vec<&'a str>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        if !seen.insert(name.to_string()) {
            return;
        }

        let Some(info) = self.classes.get(name) else {
            return;
        };

        for field in &info.fields {
            out.push(field.as_str());
        }

        // Des champs écrits uniquement via `this.x = ...` restent visibles
        // pour les anciennes classes dynamiques.
        for base in &info.bases {
            self.collect_fields(base, out, seen);
        }
    }

    fn collect_statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Positioned { statement, .. } => self.collect_statement(statement),
            Statement::Export { statement } => self.collect_statement(statement),
            Statement::Class {
                name,
                bases,
                fields,
                methods,
            } => {
                let mut field_info = HashMap::new();
                let mut field_names = Vec::new();

                for field in fields {
                    field_names.push(field.name.clone());
                    field_info.insert(
                        field.name.clone(),
                        FieldInfo {
                            name: field.name.clone(),
                            type_annotation: field.type_annotation.clone(),
                            visibility: field.visibility,
                        },
                    );
                }

                // Compatibilité avec le code qui avait des champs dynamiques
                // initialisés par `this.name = ...` sans déclaration `let`.
                for method in methods {
                    collect_this_fields(&method.body, &mut field_names);
                }

                field_names.sort();
                field_names.dedup();

                self.classes.insert(
                    name.clone(),
                    ClassInfo {
                        bases: bases.clone(),
                        methods: methods
                            .iter()
                            .filter(|method| !method.name.starts_with("__fields_"))
                            .map(|method| MethodInfo {
                                name: method.name.clone(),
                                params: method.params.clone(),
                                param_types: method.param_types.clone(),
                                return_type: method.return_type.clone(),
                                visibility: method.visibility,
                            })
                            .collect(),
                        fields: field_names,
                        field_info,
                        is_interface: false,
                    },
                );
            }
            Statement::Interface {
                name,
                bases,
                methods,
            } => {
                self.classes.insert(
                    name.clone(),
                    ClassInfo {
                        bases: bases.clone(),
                        methods: methods
                            .iter()
                            .map(|method| MethodInfo {
                                name: method.name.clone(),
                                params: method.params.clone(),
                                param_types: method.param_types.clone(),
                                return_type: method.return_type.clone(),
                                visibility: Visibility::Public,
                            })
                            .collect(),
                        fields: Vec::new(),
                        field_info: HashMap::new(),
                        is_interface: true,
                    },
                );
            }
            _ => {}
        }
    }
}

fn collect_this_fields(body: &[Statement], out: &mut Vec<String>) {
    for statement in body {
        collect_this_fields_in_statement(statement, out);
    }
}

fn collect_this_fields_in_statement(statement: &Statement, out: &mut Vec<String>) {
    match statement {
        Statement::Positioned { statement, .. } => collect_this_fields_in_statement(statement, out),
        Statement::Assignment { target, .. } => {
            if let AssignmentTarget::Member { object, name } = target {
                if matches!(object.as_ref(), Expression::This) {
                    out.push(name.clone());
                }
            }
        }
        Statement::Block(statements) => collect_this_fields(statements, out),
        Statement::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_this_fields(then_branch, out);
            if let Some(statements) = else_branch {
                collect_this_fields(statements, out);
            }
        }
        Statement::While { body, .. } | Statement::ForIn { body, .. } => {
            collect_this_fields(body, out)
        }
        Statement::Match { arms, .. } => {
            for arm in arms {
                collect_this_fields(&arm.body, out);
            }
        }
        Statement::Try {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            collect_this_fields(try_body, out);
            if let Some(statements) = catch_body {
                collect_this_fields(statements, out);
            }
            if let Some(statements) = finally_body {
                collect_this_fields(statements, out);
            }
        }
        _ => {}
    }
}

pub fn type_expr_display(expr: &TypeExpr) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use kastel::frontend::lexer::lexer::Lexer;
    use kastel::frontend::parser::Parser;

    #[test]
    fn indexes_declared_field_and_initialize() {
        let source = r#"class Person {
    private let age: int = 0;
    func initialize(age: int) -> None {
        this.age = age;
    }
    func name() -> str {
        return "Bruno";
    }
}
"#;
        let mut lexer = Lexer::new(source.to_owned());
        let tokens = lexer.scan_token().unwrap();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        let mut index = ClassIndex::new();
        index.rebuild(&statements);

        let class = index.get("Person").unwrap();
        assert!(class.fields.contains(&"age".to_string()));
        assert!(index.method("Person", "initialize").is_some());
        assert!(index.field("Person", "age").is_some());
    }
}
