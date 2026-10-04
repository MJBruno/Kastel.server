use std::collections::HashMap;

use kastel::compiler::types::{FunctionType, Type};
use kastel::frontend::ast::{BinaryOp, Expression, Literal, Statement, TypeExpr, UnaryOp};

#[derive(Debug, Clone, Default)]
pub struct TypeInfo {
    pub symbols: HashMap<String, Type>,
    pub functions: HashMap<String, Vec<FunctionSignature>>,
    pub aliases: HashMap<String, Type>,
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: String,
    /// Noms des paramètres génériques : `func premier<T>(...)` -> `["T"]`.
    pub generic_params: Vec<String>,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    /// `async func ...` (champ requis par `FunctionType` du crate kastel).
    pub is_async: bool,
}

impl FunctionSignature {
    pub fn function_type(&self) -> FunctionType {
        FunctionType {
            generic_params: self.generic_params.clone(),
            generic_constraints: Vec::new(),
            params: self.params.iter().map(|(_, ty)| ty.clone()).collect(),
            return_type: Box::new(self.return_type.clone()),
            is_async: self.is_async,
        }
    }

    pub fn label(&self) -> String {
        let generics = if self.generic_params.is_empty() {
            String::new()
        } else {
            format!("<{}>", self.generic_params.join(", "))
        };

        format!(
            "func {}{}({}) -> {}",
            self.name,
            generics,
            self.params
                .iter()
                .map(|(name, ty)| format!("{}: {}", name, ty))
                .collect::<Vec<_>>()
                .join(", "),
            self.return_type
        )
    }
}

impl TypeInfo {
    pub fn build(statements: &[Statement]) -> Self {
        let mut info = Self::default();
        let mut scopes = vec![HashMap::<String, Type>::new()];
        collect_statements(statements, &mut info, &mut scopes);
        info
    }

    pub fn get(&self, name: &str) -> Option<&Type> {
        self.symbols.get(name)
    }

    pub fn function_signatures(&self, name: &str) -> Option<&[FunctionSignature]> {
        self.functions.get(name).map(Vec::as_slice)
    }

    // pub fn infer_expression(&self, expr: &Expression) -> Type {
    //     infer_expression(self, expr, &self.symbols)
    // }

    pub fn member_type(&self, receiver: &Type, name: &str) -> Option<Type> {
        if let Some(method) = receiver.collection_member_type(name) {
            return Some(method);
        }
        if let Some(method) = receiver.record_method_type(name) {
            return Some(method);
        }
        if let Some(method) = receiver.set_member_type(name) {
            return Some(method);
        }
        // Types génériques et handles du runtime. Chaque helper renvoie `None`
        // pour un receveur d'un autre genre ; l'ordre est donc indifférent.
        receiver
            .option_result_member_type(name)
            .or_else(|| receiver.task_member_type(name))
            .or_else(|| receiver.channel_member_type(name))
            .or_else(|| receiver.mutex_member_type(name))
            .or_else(|| receiver.semaphore_member_type(name))
            .or_else(|| receiver.wait_group_member_type(name))
    }
}

fn collect_statements(
    statements: &[Statement],
    info: &mut TypeInfo,
    scopes: &mut Vec<HashMap<String, Type>>,
) {
    for raw in statements {
        let statement = unwrap(raw);
        match statement {
            Statement::Let {
                name,
                value,
                type_annotation,
                ..
            } => {
                let ty = type_annotation
                    .as_ref()
                    .map(|expr| resolve_type_expr(info, expr))
                    .unwrap_or_else(|| infer_expression(info, value, scopes.last().unwrap()));
                info.symbols.insert(name.clone(), ty.clone());
                scopes.last_mut().unwrap().insert(name.clone(), ty);
            }
            Statement::Function {
                name,
                generic_params,
                params,
                param_types,
                return_type,
                body,
                is_async,
            } => {
                let params_with_types = params
                    .iter()
                    .enumerate()
                    .map(|(i, param)| {
                        let ty = param_types
                            .get(i)
                            .and_then(|t| t.as_ref())
                            .map(|t| resolve_type_expr(info, t))
                            .unwrap_or(Type::Dynamic);
                        (param.clone(), ty)
                    })
                    .collect::<Vec<_>>();

                let inferred_return = return_type
                    .as_ref()
                    .map(|t| resolve_type_expr(info, t))
                    .unwrap_or_else(|| infer_return_type(info, body, &params_with_types));

                let signature = FunctionSignature {
                    name: name.clone(),
                    generic_params: generic_params
                        .iter()
                        .map(|param| param.name.clone())
                        .collect(),
                    params: params_with_types,
                    return_type: inferred_return,
                    is_async: is_async.clone(),
                };

                info.symbols.insert(
                    name.clone(),
                    if signature.params.is_empty() {
                        Type::Function(signature.function_type())
                    } else {
                        Type::Function(signature.function_type())
                    },
                );
                info.functions
                    .entry(name.clone())
                    .or_default()
                    .push(signature);

                let mut function_scope = HashMap::new();
                for (param, ty) in &info.functions[name].last().unwrap().params {
                    function_scope.insert(param.clone(), ty.clone());
                }
                scopes.push(function_scope);
                collect_statements(body, info, scopes);
                scopes.pop();
            }
            Statement::Class { name, .. }
            | Statement::Interface { name, .. }
            | Statement::Enum { name, .. } => {
                info.symbols.insert(name.clone(), Type::Named(name.clone()));
            }
            Statement::TypeAlias {
                name, type_expr, ..
            } => {
                let ty = resolve_type_expr(info, type_expr);
                info.aliases.insert(name.clone(), ty.clone());
                info.symbols.insert(name.clone(), ty);
            }
            Statement::Import { path } => {
                if let Some(name) = path.last() {
                    info.symbols
                        .insert(name.clone(), Type::Module(path.join(".")));
                }
            }
            Statement::FromImport { items, .. } => {
                for item in items {
                    let name = item.alias.as_ref().unwrap_or(&item.name);
                    info.symbols.insert(name.clone(), Type::Dynamic);
                }
            }
            Statement::Export { statement } => {
                collect_statements(std::slice::from_ref(statement), info, scopes);
            }
            Statement::Block(body) => {
                scopes.push(HashMap::new());
                collect_statements(body, info, scopes);
                scopes.pop();
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                scopes.push(HashMap::new());
                collect_statements(then_branch, info, scopes);
                scopes.pop();
                if let Some(branch) = else_branch {
                    scopes.push(HashMap::new());
                    collect_statements(branch, info, scopes);
                    scopes.pop();
                }
            }
            Statement::While { body, .. } => {
                scopes.push(HashMap::new());
                collect_statements(body, info, scopes);
                scopes.pop();
            }
            Statement::ForIn {
                variable,
                iterable,
                body,
            } => {
                let element_type =
                    infer_expression(info, iterable, scopes.last().unwrap()).element_type();
                let mut loop_scope = HashMap::new();
                loop_scope.insert(variable.clone(), element_type);
                scopes.push(loop_scope);
                collect_statements(body, info, scopes);
                scopes.pop();
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    scopes.push(HashMap::new());
                    collect_statements(&arm.body, info, scopes);
                    scopes.pop();
                }
            }
            Statement::Try {
                try_body,
                catch_name,
                catch_body,
                finally_body,
                ..
            } => {
                scopes.push(HashMap::new());
                collect_statements(try_body, info, scopes);
                scopes.pop();
                if let Some(body) = catch_body {
                    let mut catch_scope = HashMap::new();
                    if let Some(name) = catch_name {
                        catch_scope.insert(name.clone(), Type::Dynamic);
                    }
                    scopes.push(catch_scope);
                    collect_statements(body, info, scopes);
                    scopes.pop();
                }
                if let Some(body) = finally_body {
                    scopes.push(HashMap::new());
                    collect_statements(body, info, scopes);
                    scopes.pop();
                }
            }
            _ => {}
        }
    }
}

fn infer_return_type(info: &TypeInfo, body: &[Statement], params: &[(String, Type)]) -> Type {
    let mut env = info.symbols.clone();
    env.extend(params.iter().cloned());
    let mut result = None;
    collect_returns(info, body, &env, &mut result);
    result.unwrap_or(Type::None)
}

fn collect_returns(
    info: &TypeInfo,
    statements: &[Statement],
    env: &HashMap<String, Type>,
    result: &mut Option<Type>,
) {
    for raw in statements {
        match unwrap(raw) {
            Statement::Return { value } => {
                let ty = value
                    .as_ref()
                    .map(|expr| infer_expression(info, expr, env))
                    .unwrap_or(Type::None);
                *result = Some(match result.take() {
                    None => ty,
                    Some(previous) => previous.merge(&ty),
                });
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_returns(info, then_branch, env, result);
                if let Some(branch) = else_branch {
                    collect_returns(info, branch, env, result);
                }
            }
            Statement::While { body, .. }
            | Statement::ForIn { body, .. }
            | Statement::Block(body) => {
                collect_returns(info, body, env, result);
            }
            Statement::Try {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                collect_returns(info, try_body, env, result);
                if let Some(body) = catch_body {
                    collect_returns(info, body, env, result);
                }
                if let Some(body) = finally_body {
                    collect_returns(info, body, env, result);
                }
            }
            _ => {}
        }
    }
}

fn infer_expression(info: &TypeInfo, expr: &Expression, env: &HashMap<String, Type>) -> Type {
    match expr {
        Expression::Literal(literal) => match literal {
            Literal::Integer(_) => Type::Int,
            Literal::Float(_) => Type::Float,
            Literal::String(_) => Type::Str,
            Literal::Bool(_) => Type::Bool,
            Literal::None => Type::None,
        },
        Expression::Variable(name) => env
            .get(name)
            .cloned()
            .or_else(|| info.symbols.get(name).cloned())
            .unwrap_or(Type::Dynamic),
        Expression::SelfValue => Type::Dynamic,
        Expression::New {
            class_name,
            generic_args,
            ..
        } => {
            if generic_args.is_empty() {
                Type::Named(class_name.clone())
            } else {
                Type::Generic {
                    name: class_name.clone(),
                    arguments: generic_args
                        .iter()
                        .map(|arg| resolve_type_expr(info, arg))
                        .collect(),
                }
            }
        }
        // Opérateur `?` : déballe `Option<T>` / `Result<T, E>` en `T`.
        Expression::Try(inner) => match infer_expression(info, inner, env) {
            Type::Generic { name, arguments }
                if matches!(name.to_ascii_lowercase().as_str(), "option" | "result") =>
            {
                arguments.into_iter().next().unwrap_or(Type::Dynamic)
            }
            _ => Type::Dynamic,
        },
        Expression::Array(items) => {
            let element = items
                .iter()
                .map(|item| infer_expression(info, item, env))
                .reduce(|a, b| a.merge(&b))
                .unwrap_or(Type::Dynamic);
            Type::Array(Box::new(element))
        }
        Expression::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| infer_expression(info, item, env))
                .collect(),
        ),
        Expression::Dict(items) => {
            let value = items
                .iter()
                .map(|(_, value)| infer_expression(info, value, env))
                .reduce(|a, b| a.merge(&b))
                .unwrap_or(Type::Dynamic);
            Type::Dict(Box::new(Type::Str), Box::new(value))
        }
        Expression::Record(fields) => Type::Record(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), infer_expression(info, value, env)))
                .collect(),
        ),
        Expression::Unary {
            operator, right, ..
        } => match operator {
            UnaryOp::Not => Type::Bool,
            UnaryOp::Negate | UnaryOp::BitNot => infer_expression(info, right, env),
        },
        Expression::Binary {
            left,
            operator,
            right,
            ..
        } => {
            let left_ty = infer_expression(info, left, env);
            let right_ty = infer_expression(info, right, env);
            match operator {
                BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
                | BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Is => Type::Bool,
                BinaryOp::Add => {
                    if left_ty == Type::Str || right_ty == Type::Str {
                        Type::Str
                    } else {
                        left_ty.merge(&right_ty)
                    }
                }
                BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Modulo
                | BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight => left_ty.merge(&right_ty),
            }
        }
        Expression::Member { object, name, .. } => {
            let object_ty = infer_expression(info, object, env);
            info.member_type(&object_ty, name).unwrap_or(Type::Dynamic)
        }
        Expression::Index { object, .. } => infer_expression(info, object, env).element_type(),
        Expression::Call {
            callee, arguments, ..
        } => {
            if let Some(ty) = infer_intrinsic_call(info, callee, arguments, env) {
                return ty;
            }
            match infer_expression(info, callee, env) {
            Type::Function(function) => (*function.return_type).clone(),
            Type::Overloads(overloads) => overloads
                .first()
                .map(|f| (*f.return_type).clone())
                .unwrap_or(Type::Dynamic),
            other => other,
            }
        }
        Expression::Function { params, .. } => Type::Function(FunctionType {
            generic_params: Vec::new(),
            generic_constraints: Vec::new(),
            params: vec![Type::Dynamic; params.len()],
            return_type: Box::new(Type::Dynamic),
            is_async: false,
        }),
        Expression::Ternary {
            then_expr,
            else_expr,
            ..
        } => infer_expression(info, then_expr, env).merge(&infer_expression(info, else_expr, env)),
        // `await f()` : la valeur attendue a le type de retour de la fonction async.
        Expression::Await(inner) => infer_expression(info, inner, env),
    }
}

/// Types produits par les natives et intrinsèques du runtime (`Some`, `Ok`,
/// `Err`, `spawn`, `channel`, `mutex`, `semaphore`, `wait_group`).
///
/// Un identifiant redéfini par l'utilisateur (variable ou fonction du même
/// nom) reprend la priorité : on ne déduit rien dans ce cas.
fn infer_intrinsic_call(
    info: &TypeInfo,
    callee: &Expression,
    arguments: &[Expression],
    env: &HashMap<String, Type>,
) -> Option<Type> {
    let Expression::Variable(name) = callee else {
        return None;
    };
    if env.contains_key(name) || info.symbols.contains_key(name) {
        return None;
    }

    let generic = |name: &str, arguments: Vec<Type>| Type::Generic {
        name: name.to_string(),
        arguments,
    };
    let first = || {
        arguments
            .first()
            .map(|arg| infer_expression(info, arg, env))
            .unwrap_or(Type::Dynamic)
    };

    Some(match name.as_str() {
        "Some" => generic("Option", vec![first()]),
        "Ok" => generic("Result", vec![first(), Type::Dynamic]),
        "Err" => generic("Result", vec![Type::Dynamic, first()]),
        "spawn" => {
            let result = match first() {
                Type::Function(function) => (*function.return_type).clone(),
                _ => Type::Dynamic,
            };
            generic("Task", vec![result])
        }
        "channel" => generic("Channel", vec![Type::Dynamic]),
        "mutex" => Type::Named("Mutex".to_string()),
        "semaphore" => Type::Named("Semaphore".to_string()),
        "wait_group" => Type::Named("WaitGroup".to_string()),
        _ => return None,
    })
}

fn resolve_type_expr(info: &TypeInfo, expr: &TypeExpr) -> Type {
    let ty = Type::from_type_expr(expr);
    match ty {
        Type::Named(ref name) if info.aliases.contains_key(name) => info.aliases[name].clone(),
        other => other,
    }
}

fn unwrap(statement: &Statement) -> &Statement {
    match statement {
        Statement::Positioned { statement, .. } => unwrap(statement),
        _ => statement,
    }
}
