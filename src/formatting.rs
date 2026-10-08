//! Formateur Kastel : `textDocument/formatting`.
//!
//! Le formateur reste syntaxique (et non AST -> source) pour conserver les
//! commentaires et fonctionner pendant la frappe. Il fait deux passes :
//!
//! 1. indentation selon `{ } ( ) [ ]` ;
//! 2. normalisation légère des espaces autour des opérateurs et séparateurs.
//!
//! Le style produit vise un code Kastel compact mais aéré :
//! - `self.nom = nom;`
//! - `2 * 3`
//! - `(5, 5)`
//! - `let x: str = "hello";`
//! - une seule ligne vide entre déclarations de haut niveau et méthodes.
//!
//! Les chaînes et commentaires sont traités comme du contenu opaque.

use serde_json::{Value, json};

use crate::lsp_position::offset_to_lsp;
use crate::text_util::mask_strings_and_comments;
use crate::workspace::Workspace;

/// Construit la liste de `TextEdit` LSP pour reformater tout le document.
pub fn build_formatting(
    workspace: &Workspace,
    uri: &str,
    tab_size: u32,
    insert_spaces: bool,
) -> Option<Value> {
    let document = workspace.get(uri)?;

    let formatted = format_source(&document.text, tab_size, insert_spaces);

    if formatted == document.text {
        return Some(Value::Array(Vec::new()));
    }

    let (end_line, end_character) = offset_to_lsp(&document.text, document.text.len());

    Some(json!([
        {
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": end_line, "character": end_character }
            },
            "newText": formatted
        }
    ]))
}

/// Reformate une source Kastel complète.
pub fn format_source(source: &str, tab_size: u32, insert_spaces: bool) -> String {
    let indent_unit = if insert_spaces {
        " ".repeat(tab_size.max(1) as usize)
    } else {
        "\t".to_string()
    };

    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");

    // `return a;}` -> `return a;` puis `}` quand l'accolade ferme un bloc ouvert plus haut.
    let mut split_comment_state = false;
    let raw_lines: Vec<String> = normalized
        .split('\n')
        .flat_map(|line| split_stray_closing_braces(line, &mut split_comment_state))
        .collect();

    let mut depth: i64 = 0;
    let mut in_block_comment = false;
    let mut output: Vec<String> = Vec::with_capacity(raw_lines.len() + 8);
    let mut previous_non_blank: Option<usize> = None;

    for raw_line in raw_lines {
        let trimmed = raw_line.trim();

        if trimmed.is_empty() {
            output.push(String::new());
            continue;
        }

        let formatted_content = normalize_line(&raw_line, &mut in_block_comment);
        let content = formatted_content.trim_end();

        if content.is_empty() {
            output.push(String::new());
            continue;
        }

        let leading_closers = count_leading_closers(content);
        let print_depth = (depth - leading_closers as i64).max(0);

        // Aération volontairement légère : au maximum une ligne vide entre
        // déclarations de haut niveau, et entre méthodes d'une même classe.
        if let Some(previous_index) = previous_non_blank {
            if should_insert_blank_line(&output[previous_index], content, print_depth, depth) {
                if output.last().is_none_or(|line| !line.trim().is_empty()) {
                    output.push(String::new());
                }
            }
        }

        let mut line =
            String::with_capacity(content.len() + print_depth as usize * indent_unit.len());
        for _ in 0..print_depth {
            line.push_str(&indent_unit);
        }
        line.push_str(content);

        output.push(line);
        previous_non_blank = Some(output.len() - 1);

        depth = (depth + bracket_delta(&mask_strings_and_comments(content))).max(0);
    }

    let output = collapse_blank_lines(output);
    let mut result = output.join("\n");

    if !result.is_empty() {
        result.push('\n');
    }

    result
}

/// Sépare les `}` qui ferment un bloc ouvert sur une ligne précédente lorsqu'ils
/// suivent du code sur la même ligne :
///
/// ```text
///     if a == b {
///         return a;}          ->   return a;   puis   }
/// ```
///
/// Les accolades équilibrées sur une même ligne (`if a == b {return a;}`,
/// `{ a: 1 }`) ne sont jamais touchées, pas plus que celles placées dans une
/// chaîne ou un commentaire. Un `}` en début de ligne (`} else {`) reste en place.
fn split_stray_closing_braces(line: &str, in_block_comment: &mut bool) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut cuts: Vec<usize> = Vec::new();
    let mut depth = 0usize;
    let mut has_code = false;
    let mut i = 0usize;

    while i < chars.len() {
        if *in_block_comment {
            if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                *in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        match chars[i] {
            '"' | '\'' => {
                let quote = chars[i];
                has_code = true;
                i += 1;

                while i < chars.len() {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 2;
                        continue;
                    }

                    i += 1;

                    if chars[i - 1] == quote {
                        break;
                    }
                }
            }
            '/' if i + 1 < chars.len() && chars[i + 1] == '/' => break,
            '/' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                *in_block_comment = true;
                i += 2;
            }
            '{' => {
                depth += 1;
                has_code = true;
                i += 1;
            }
            '}' => {
                if depth > 0 {
                    depth -= 1;
                    has_code = true;
                } else if has_code {
                    // Ferme un bloc ouvert plus haut, après du code : nouvelle ligne ici.
                    cuts.push(i);
                    has_code = false;
                }

                i += 1;
            }
            character => {
                if !character.is_whitespace() {
                    has_code = true;
                }

                i += 1;
            }
        }
    }

    if cuts.is_empty() {
        return vec![line.to_string()];
    }

    let mut parts = Vec::with_capacity(cuts.len() + 1);
    let mut start = 0usize;

    for cut in cuts {
        parts.push(chars[start..cut].iter().collect::<String>());
        start = cut;
    }

    parts.push(chars[start..].iter().collect::<String>());

    parts
}

/// Normalise une ligne sans modifier le contenu exact des chaînes et commentaires.
/// Ces éléments sont remplacés temporairement par des identifiants sentinelles,
/// puis restaurés après la normalisation des espaces.
fn normalize_line(line: &str, in_block_comment: &mut bool) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut code = String::new();
    let mut opaque: Vec<String> = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        if *in_block_comment {
            let start = i;
            while i + 1 < chars.len() {
                if chars[i] == '*' && chars[i + 1] == '/' {
                    i += 2;
                    *in_block_comment = false;
                    break;
                }
                i += 1;
            }

            let end = i;
            let index = opaque.len();
            opaque.push(chars[start..end].iter().collect());
            code.push_str(&format!("__KASTEL_FMT_TOKEN_{index}__"));
            continue;
        }

        match chars[i] {
            '"' | '\'' => {
                let quote = chars[i];
                let start = i;
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    if chars[i - 1] == quote {
                        break;
                    }
                }

                let index = opaque.len();
                opaque.push(chars[start..i].iter().collect());
                code.push_str(&format!("__KASTEL_FMT_TOKEN_{index}__"));
            }
            '/' if i + 1 < chars.len() && chars[i + 1] == '/' => {
                let index = opaque.len();
                opaque.push(chars[i..].iter().collect());
                code.push_str(&format!("__KASTEL_FMT_TOKEN_{index}__"));
                break;
            }
            '/' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                let start = i;
                i += 2;
                while i + 1 < chars.len() {
                    if chars[i] == '*' && chars[i + 1] == '/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }

                if i >= chars.len() && !(i >= 2 && chars[i - 2] == '*' && chars[i - 1] == '/') {
                    *in_block_comment = true;
                }

                let index = opaque.len();
                opaque.push(chars[start..i.min(chars.len())].iter().collect());
                code.push_str(&format!("__KASTEL_FMT_TOKEN_{index}__"));
            }
            _ => {
                code.push(chars[i]);
                i += 1;
            }
        }
    }

    let mut formatted = normalize_code_segment(&code);

    for (index, original) in opaque.iter().enumerate() {
        let placeholder = format!("__KASTEL_FMT_TOKEN_{index}__");
        formatted = formatted.replace(&placeholder, original);
    }

    formatted.trim().to_string()
}

/// Tokenise légèrement une portion de code et reconstruit les espaces.
/// Cette fonction ne connaît pas l'AST Kastel ; elle est donc tolérante aux
/// erreurs de syntaxe pendant la frappe dans VS Code.
fn normalize_code_segment(code: &str) -> String {
    let tokens = tokenize_code(code);
    if tokens.is_empty() {
        return String::new();
    }

    let mut out = String::new();

    for index in 0..tokens.len() {
        let token = &tokens[index];
        let previous = index.checked_sub(1).and_then(|i| tokens.get(i));
        let next = tokens.get(index + 1);

        if let Some(prev) = previous {
            if needs_space_between(prev, token, next, index, &tokens) {
                out.push(' ');
            }
        }

        out.push_str(&token.text);
    }

    out.trim().to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Word,
    Number,
    String,
    Operator,
    Comma,
    Colon,
    Semicolon,
    Dot,
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    OpenBrace,
    CloseBrace,
    Other,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    text: String,
    /// Un espace précédait ce jeton dans la source (sert à conserver le
    /// remplissage des blocs `{ … }` écrits sur une seule ligne).
    space_before: bool,
}

fn tokenize_code(code: &str) -> Vec<Token> {
    let chars: Vec<char> = code.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    let mut pending_space = false;

    while i < chars.len() {
        let c = chars[i];

        if c.is_whitespace() {
            pending_space = true;
            i += 1;
            continue;
        }

        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(Token {
                space_before: std::mem::take(&mut pending_space),
                kind: TokenKind::Word,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        if c.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '.' | '_'))
            {
                i += 1;
            }
            tokens.push(Token {
                space_before: std::mem::take(&mut pending_space),
                kind: TokenKind::Number,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        if c == '"' || c == '\'' {
            let start = i;
            let quote = c;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 2;
                    continue;
                }
                i += 1;
                if chars[i - 1] == quote {
                    break;
                }
            }
            tokens.push(Token {
                space_before: std::mem::take(&mut pending_space),
                kind: TokenKind::String,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        let two = if i + 1 < chars.len() {
            Some(format!("{}{}", chars[i], chars[i + 1]))
        } else {
            None
        };
        let three = if i + 2 < chars.len() {
            Some(format!("{}{}{}", chars[i], chars[i + 1], chars[i + 2]))
        } else {
            None
        };

        let operator = match three.as_deref() {
            Some("===") | Some("!==") => three.clone(),
            _ => match two.as_deref() {
                Some("==") | Some("!=") | Some("<=") | Some(">=") | Some("&&") | Some("||")
                | Some("->") | Some("+=") | Some("-=") | Some("*=") | Some("/=") | Some("%=")
                | Some("=>") | Some("??") => two.clone(),
                _ => None,
            },
        };

        if let Some(text) = operator {
            let len = text.chars().count();
            tokens.push(Token {
                space_before: std::mem::take(&mut pending_space),
                kind: TokenKind::Operator,
                text,
            });
            i += len;
            continue;
        }

        let kind = match c {
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            ';' => TokenKind::Semicolon,
            '.' => TokenKind::Dot,
            '(' => TokenKind::OpenParen,
            ')' => TokenKind::CloseParen,
            '[' => TokenKind::OpenBracket,
            ']' => TokenKind::CloseBracket,
            '{' => TokenKind::OpenBrace,
            '}' => TokenKind::CloseBrace,
            '+' | '-' | '*' | '/' | '%' | '=' | '<' | '>' | '!' | '&' | '|' | '?' => {
                TokenKind::Operator
            }
            _ => TokenKind::Other,
        };

        tokens.push(Token {
                space_before: std::mem::take(&mut pending_space),
            kind,
            text: c.to_string(),
        });
        i += 1;
    }

    split_generic_close_equal(tokens)
}

/// `let v: List<int>=[]` : le lexer produit `>=` ; si le `>` ferme un générique,
/// on le sépare en `>` et `=`.
fn split_generic_close_equal(tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len() + 2);

    for token in tokens {
        if token.kind == TokenKind::Operator && token.text == ">=" && closes_open_generic(&out) {
            out.push(Token {
                kind: TokenKind::Operator,
                text: ">".to_string(),
                space_before: token.space_before,
            });
            out.push(Token {
                kind: TokenKind::Operator,
                text: "=".to_string(),
                space_before: false,
            });
        } else {
            out.push(token);
        }
    }

    out
}

/// Vrai s'il reste un `<` de paramètres de type ouvert (non refermé) dans `tokens`.
fn closes_open_generic(tokens: &[Token]) -> bool {
    let mut depth = 0usize;

    for index in (0..tokens.len()).rev() {
        let token = &tokens[index];

        match token.kind {
            TokenKind::Semicolon | TokenKind::OpenBrace | TokenKind::CloseBrace => return false,
            TokenKind::Operator if token.text == ">" => depth += 1,
            TokenKind::Operator if token.text == "<" => {
                if depth == 0 {
                    return index >= 1
                        && tokens[index - 1].kind == TokenKind::Word
                        && (is_generic_head(&tokens[index - 1].text)
                            || follows_declaration_keyword(index, tokens));
                }

                depth -= 1;
            }
            _ => {}
        }
    }

    false
}

fn needs_space_between(
    previous: &Token,
    current: &Token,
    next: Option<&Token>,
    current_index: usize,
    tokens: &[Token],
) -> bool {
    // Accès membre et appels/indexation.
    match (previous.kind, current.kind) {
        (TokenKind::Dot, _) | (_, TokenKind::Dot) => return false,
        (TokenKind::OpenParen, _) | (_, TokenKind::CloseParen) => return false,
        (TokenKind::OpenBracket, _) | (_, TokenKind::CloseBracket) => return false,
        _ => {}
    }

    // Bloc `{ … }` sur une seule ligne : l'espace après `{` et avant `}` est conservé
    // tel qu'il est écrit (`{return a;}` reste `{return a;}`, `{ a: 1 }` reste `{ a: 1 }`).
    if previous.kind == TokenKind::OpenBrace || current.kind == TokenKind::CloseBrace {
        return current.space_before;
    }

    // Ternaire : `cond ? a : b` (le `:` d'une annotation reste collé : `x: int`).
    if current.kind == TokenKind::Colon && colon_is_ternary(tokens, current_index) {
        return true;
    }

    // Les génériques restent compacts : `List<int>`, `Dict<str, int>`,
    // `Map<str, List<int>>`, etc. Les comparaisons restent espacées : `a < b`.
    if current_is_generic_angle(current, previous, next, current_index, tokens)
        || current_is_generic_close(current, current_index, tokens)
        || previous_is_generic_close(previous, current, current_index, tokens)
    {
        return false;
    }

    // `for x in [1, 2]`, `return [a]`, `if (a)`, `catch (e: Err)` : un mot-clé est
    // séparé de `[` et `(` (contrairement à un appel `f(x)` ou une indexation `a[0]`).
    if previous.kind == TokenKind::Word
        && matches!(current.kind, TokenKind::OpenParen | TokenKind::OpenBracket)
        && is_keyword_before_value(&previous.text)
    {
        return true;
    }

    if current.kind == TokenKind::Operator {
        if is_unary_operator(tokens, current_index) {
            // `= -5`, `* -2`, `, -1`, `return -x`, `&& !ok`, mais `!!x` et `(-1)`.
            return match previous.kind {
                TokenKind::Operator => !is_unary_operator(tokens, current_index - 1),
                TokenKind::Comma | TokenKind::Colon => true,
                TokenKind::Word => is_keyword_before_value(&previous.text),
                _ => false,
            };
        }

        return true;
    }

    // Aucun espace après l'ouverture d'un générique : `Dict<str, int>`.
    if previous.text == "<" && is_generic_open_at(current_index - 1, tokens) {
        return false;
    }

    if previous.kind == TokenKind::Operator {
        return !is_unary_operator(tokens, current_index - 1);
    }

    match (previous.kind, current.kind) {
        // Séparateurs.
        (_, TokenKind::Comma) | (_, TokenKind::Colon) | (_, TokenKind::Semicolon) => false,
        (TokenKind::Comma, _) | (TokenKind::Colon, _) => true,
        (TokenKind::Semicolon, _) => true,

        // Blocs : `func f() {`, `} else`.
        (_, TokenKind::OpenBrace) => true,
        (TokenKind::CloseBrace, TokenKind::Word) => true,

        // Mots/littéraux adjacents : `return value`, `new Person`, etc.
        (
            TokenKind::Word | TokenKind::Number | TokenKind::String,
            TokenKind::Word | TokenKind::Number | TokenKind::String,
        ) => true,

        (TokenKind::CloseParen | TokenKind::CloseBracket, TokenKind::Word) => true,

        _ => false,
    }
}

/// Mots-clés qui sont suivis d'une valeur ou d'une expression (et non d'un appel).
fn is_keyword_before_value(text: &str) -> bool {
    matches!(
        text,
        "in" | "return"
            | "if"
            | "while"
            | "for"
            | "match"
            | "else"
            | "is"
            | "await"
            | "throw"
            | "catch"
    )
}

fn is_unary_operator(tokens: &[Token], index: usize) -> bool {
    let token = match tokens.get(index) {
        Some(token) => token,
        None => return false,
    };

    match token.text.as_str() {
        "!" => true,
        "+" | "-" => {
            let previous = index.checked_sub(1).and_then(|i| tokens.get(i));
            match previous {
                None => true,
                Some(previous) => {
                    matches!(
                        previous.kind,
                        TokenKind::Operator
                            | TokenKind::OpenParen
                            | TokenKind::OpenBracket
                            | TokenKind::OpenBrace
                            | TokenKind::Comma
                            | TokenKind::Colon
                    ) || (previous.kind == TokenKind::Word && is_keyword_before_value(&previous.text))
                }
            }
        }
        _ => false,
    }
}

fn is_generic_open_at(index: usize, tokens: &[Token]) -> bool {
    if index >= tokens.len() || tokens[index].text != "<" || index == 0 {
        return false;
    }

    let previous = &tokens[index - 1];
    let next = tokens.get(index + 1);
    current_is_generic_angle(&tokens[index], previous, next, index, tokens)
}

fn current_is_generic_angle(
    current: &Token,
    previous: &Token,
    next: Option<&Token>,
    current_index: usize,
    tokens: &[Token],
) -> bool {
    if current.text != "<" || previous.kind != TokenKind::Word {
        return false;
    }

    let Some(next) = next else { return false };
    if !matches!(next.kind, TokenKind::Word | TokenKind::Operator) || next.text == ">" {
        return false;
    }

    // Seuls les mots qui peuvent introduire un paramétrage de type ouvrent
    // ici un groupe générique. Cela permet notamment `Dict<str, int>` et
    // `List<int>` sans transformer `a < b` en `a<b`.
    if !is_generic_head(previous.text.as_str()) && !follows_declaration_keyword(current_index, tokens)
    {
        return false;
    }

    let mut depth = 0usize;
    for token in tokens.iter().skip(current_index + 1) {
        match token.text.as_str() {
            "<" => depth += 1,
            ">" => {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            }
            _ if depth == 0
                && matches!(
                    token.kind,
                    TokenKind::Semicolon | TokenKind::OpenBrace | TokenKind::CloseBrace
                ) =>
            {
                return false;
            }
            _ => {}
        }
    }

    false
}

/// `func plus_grand<T: Ord>(…)`, `class Boite<T>` : le mot placé avant `<` suit un
/// mot-clé de déclaration, donc `<` ouvre des paramètres de type, même si le nom
/// est en minuscules (fonctions en snake_case).
fn follows_declaration_keyword(angle_index: usize, tokens: &[Token]) -> bool {
    angle_index >= 2
        && matches!(
            tokens[angle_index - 2].text.as_str(),
            "func" | "class" | "interface" | "enum" | "type"
        )
}

/// `:` appartenant à un ternaire `cond ? a : b` (et non à une annotation
/// `x: int` ni à une clé de record) : il prend un espace avant lui.
fn colon_is_ternary(tokens: &[Token], colon_index: usize) -> bool {
    let mut depth = 0usize;
    let mut pending_colons = 0usize;

    for index in (0..colon_index).rev() {
        let token = &tokens[index];

        match token.kind {
            TokenKind::CloseParen | TokenKind::CloseBracket | TokenKind::CloseBrace => depth += 1,
            TokenKind::OpenParen | TokenKind::OpenBracket | TokenKind::OpenBrace => {
                if depth == 0 {
                    return false;
                }

                depth -= 1;
            }
            _ if depth > 0 => {}
            TokenKind::Comma | TokenKind::Semicolon => return false,
            TokenKind::Colon => pending_colons += 1,
            TokenKind::Operator if token.text == "?" => {
                if pending_colons == 0 {
                    return true;
                }

                pending_colons -= 1;
            }
            _ => {}
        }
    }

    false
}

fn is_generic_head(name: &str) -> bool {
    matches!(
        name,
        "List"
            | "Dict"
            | "Set"
            | "Tuple"
            | "Range"
            | "Option"
            | "Result"
            | "Map"
            | "Future"
            | "Promise"
            | "Iterator"
            | "Iterable"
    ) || name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn current_is_generic_close(current: &Token, current_index: usize, tokens: &[Token]) -> bool {
    if current.text != ">" || current_index == 0 {
        return false;
    }

    let mut depth = 0usize;
    for open_index in (0..current_index).rev() {
        let token = &tokens[open_index];

        match token.text.as_str() {
            ">" => depth += 1,
            "<" => {
                if depth > 0 {
                    depth -= 1;
                    continue;
                }

                let open_previous = open_index
                    .checked_sub(1)
                    .and_then(|index| tokens.get(index));
                let open_next = tokens.get(open_index + 1);

                return match (open_previous, open_next) {
                    (Some(previous), Some(next)) => {
                        current_is_generic_angle(token, previous, Some(next), open_index, tokens)
                    }
                    _ => false,
                };
            }
            _ if depth == 0
                && matches!(
                    token.kind,
                    TokenKind::Semicolon | TokenKind::OpenBrace | TokenKind::CloseBrace
                ) =>
            {
                break;
            }
            _ => {}
        }
    }

    false
}

fn previous_is_generic_close(
    previous: &Token,
    current: &Token,
    current_index: usize,
    tokens: &[Token],
) -> bool {
    if previous.text != ">" || current_index == 0 {
        return false;
    }

    if !matches!(
        current.kind,
        TokenKind::OpenParen | TokenKind::OpenBracket | TokenKind::Dot
    ) {
        return false;
    }

    current_is_generic_close(previous, current_index - 1, tokens)
}

/// Compte les fermetures (`}`, `)`, `]`) consécutives en tête de ligne.
fn count_leading_closers(line: &str) -> usize {
    line.chars()
        .take_while(|c| matches!(c, '}' | ')' | ']'))
        .count()
}

/// Delta net d'ouvertures/fermetures sur une ligne déjà masquée des chaînes.
fn bracket_delta(masked: &str) -> i64 {
    let mut delta = 0i64;

    for c in masked.chars() {
        match c {
            '{' | '(' | '[' => delta += 1,
            '}' | ')' | ']' => delta -= 1,
            _ => {}
        }
    }

    delta
}

fn is_declaration_start(line: &str) -> bool {
    let mut words = line.split_whitespace();
    let first = words.next().unwrap_or_default();
    let second = words.next().unwrap_or_default();

    matches!(
        (first, second),
        ("func", _)
            | ("async", "func")
            | ("class", _)
            | ("interface", _)
            | ("enum", _)
            | ("type", _)
            | ("export", "func")
            | ("export", "async")
            | ("export", "class")
            | ("export", "interface")
            | ("export", "enum")
            | ("export", "type")
    )
}

fn should_insert_blank_line(
    previous_line: &str,
    current_line: &str,
    print_depth: i64,
    depth_before_line: i64,
) -> bool {
    let previous = previous_line.trim();
    let current = current_line.trim();

    if previous.is_empty() || current.is_empty() {
        return false;
    }

    // Jamais de ligne vide immédiatement après l'ouverture d'un bloc.
    if previous.ends_with('{') {
        return false;
    }

    // Déclarations de haut niveau : une seule ligne vide.
    if depth_before_line == 0 && print_depth == 0 && is_declaration_start(current) {
        return true;
    }

    // Méthodes d'une classe/interface : une seule ligne vide, mais pas avant
    // la première méthode du bloc.
    if depth_before_line > 0
        && current.starts_with("func ")
        && (previous.starts_with("func ") || previous.ends_with('}'))
    {
        return true;
    }

    false
}

/// Réduit les lignes vides consécutives à une seule, et retire les lignes
/// vides en tête/fin de fichier.
fn collapse_blank_lines(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());

    for line in lines {
        let is_blank = line.trim().is_empty();

        if is_blank
            && out
                .last()
                .is_some_and(|previous: &String| previous.trim().is_empty())
        {
            continue;
        }

        out.push(line);
    }

    while out.first().is_some_and(|line| line.trim().is_empty()) {
        out.remove(0);
    }

    while out.last().is_some_and(|line| line.trim().is_empty()) {
        out.pop();
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_common_kastel_spacing() {
        let source = r#"func demo(){
self.nom=nom;
let x:            str="hello";
let y=2*3;
let point=(5,5);
}
"#;

        let formatted = format_source(source, 4, true);

        let expected = r#"func demo() {
    self.nom = nom;
    let x: str = "hello";
    let y = 2 * 3;
    let point = (5, 5);
}
"#;

        assert_eq!(formatted, expected);
    }

    #[test]
    fn reindents_nested_blocks() {
        let source = "func add(a, b) {\nreturn a + b;\n}\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(formatted, "func add(a, b) {\n    return a + b;\n}\n");
    }

    #[test]
    fn preserves_spacing_around_opaque_strings() {
        let source = "let x=\"hello\"+\"world\";\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(formatted, "let x = \"hello\" + \"world\";\n");
    }

    #[test]
    fn keeps_comparisons_spaced() {
        let source = r#"let x=a<b;
let y=a>b;
let z=a<b&&b>c;
"#;

        let formatted = format_source(source, 4, true);

        assert_eq!(
            formatted,
            "let x = a < b;
let y = a > b;
let z = a < b && b > c;
"
        );
    }

    #[test]
    fn keeps_generic_function_declarations_compact() {
        let source = "func additionner<T:Add>(a:T,b:T)->T{\nreturn a+b;\n}\n\nfunc plus_grand<T: Ord>(a: T, b: T) -> T {\nreturn a > b ? a : b;\n}\n\nfunc somme<T:Add+Eq>(a:T,b:T)->T {\nif a == b {\nreturn a;\n}\nreturn a + b;\n}\n";

        let formatted = format_source(source, 4, true);

        for expected in [
            "func additionner<T: Add>(a: T, b: T) -> T {",
            "func plus_grand<T: Ord>(a: T, b: T) -> T {",
            "func somme<T: Add + Eq>(a: T, b: T) -> T {",
            "    return a > b ? a : b;",
            "    if a == b {",
        ] {
            assert!(formatted.contains(expected), "manque `{expected}` dans :\n{formatted}");
        }
    }

    #[test]
    fn keeps_balanced_one_line_blocks_untouched() {
        let source = "func f() {\n    if a == b {return a;}\n    let r = { a: 1 };\n}\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(formatted, source);
    }

    #[test]
    fn moves_stray_closing_brace_to_its_own_line() {
        let source = "func f() {\n    if a == b {\n        return a;}\n}\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(
            formatted,
            "func f() {\n    if a == b {\n        return a;\n    }\n}\n"
        );
    }

    #[test]
    fn moves_stray_closing_braces_around_else() {
        let source = "func f() {\n    if a {\n        x();} else {\n        y();}\n}\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(
            formatted,
            "func f() {\n    if a {\n        x();\n    } else {\n        y();\n    }\n}\n"
        );
    }

    #[test]
    fn ignores_braces_in_strings_and_comments() {
        let source = "func f() {\n    let s = \"}\";\n    g(); // }\n}\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(formatted, source);
    }

    #[test]
    fn splits_stray_closing_brace_of_multiline_record() {
        let source = "let r = {\n    a: 1,\n    b: 2};\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(formatted, "let r = {\n    a: 1,\n    b: 2\n};\n");
    }

    #[test]
    fn separates_keywords_from_brackets_and_parentheses() {
        let source = "for item in[1,2,3,5]{}\nif(a){}\nreturn[1];\nreturn -x;\nlet l = items[0];\nlet n = f(1);\n";

        let formatted = format_source(source, 4, true);

        assert!(formatted.contains("for item in [1, 2, 3, 5] {}"), "{formatted}");
        assert!(formatted.contains("if (a) {}"), "{formatted}");
        assert!(formatted.contains("return [1];"), "{formatted}");
        assert!(formatted.contains("return -x;"), "{formatted}");
        assert!(formatted.contains("let l = items[0];"), "{formatted}");
        assert!(formatted.contains("let n = f(1);"), "{formatted}");
    }

    #[test]
    fn keeps_generic_class_and_ternary_spacing() {
        let source = "class Boite<T> {\n}\nlet v = flag?1:2;\nlet w: int = flag ? 1 : 2;\nlet r = { a: 1 };\n";

        let formatted = format_source(source, 4, true);

        assert!(formatted.contains("class Boite<T> {"), "{formatted}");
        assert!(formatted.contains("let v = flag ? 1 : 2;"), "{formatted}");
        assert!(formatted.contains("let w: int = flag ? 1 : 2;"), "{formatted}");
        assert!(formatted.contains("let r = { a: 1 };"), "{formatted}");
    }

    #[test]
    fn keeps_generic_types_compact() {
        let source = "let values:List<int|float>=[];\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(formatted, "let values: List<int | float> = [];\n");
    }

    #[test]
    fn formats_type_annotations_and_return_types() {
        let source = "func add(a:int,b:int)->int{\nreturn a+b;\n}\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(
            formatted,
            "func add(a: int, b: int) -> int {\n    return a + b;\n}\n"
        );
    }

    #[test]
    fn adds_one_blank_line_between_methods() {
        let source =
            "class Point {\nfunc first() {\nreturn 1;\n}\nfunc second() {\nreturn 2;\n}\n}\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(
            formatted,
            "class Point {\n    func first() {\n        return 1;\n    }\n\n    func second() {\n        return 2;\n    }\n}\n"
        );
    }

    #[test]
    fn handles_else_dedent_then_reindent() {
        let source = "if x {\nfoo();\n} else {\nbar();\n}\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(formatted, "if x {\n    foo();\n} else {\n    bar();\n}\n");
    }

    #[test]
    fn ignores_braces_inside_strings_and_comments() {
        let source = "func f() {\nlet s = \"{ not a brace }\"; // { comment }\nreturn s;\n}\n";

        let formatted = format_source(source, 4, true);

        assert!(formatted.contains("let s = \"{ not a brace }\"; // { comment }"));
        assert!(formatted.ends_with("return s;\n}\n"));
    }

    #[test]
    fn collapses_multiple_blank_lines() {
        let source = "let a = 1;\n\n\n\nlet b = 2;\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(formatted, "let a = 1;\n\nlet b = 2;\n");
    }

    #[test]
    fn adds_light_aeration_between_top_level_declarations() {
        let source = "func a(){return 1;}\nfunc b(){return 2;}\nclass Point{}\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(
            formatted,
            "func a() {return 1;}\n\nfunc b() {return 2;}\n\nclass Point {}\n"
        );
    }

    #[test]
    fn keeps_negative_numbers_compact() {
        let source = "let x=-5;\nlet y=a*-2;\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(formatted, "let x = -5;\nlet y = a * -2;\n");
    }

    #[test]
    fn respects_tab_size_and_insert_spaces() {
        let source = "class Point {\nfunc initialize() {\nreturn 1;\n}\n}\n";

        let two_spaces = format_source(source, 2, true);

        assert!(two_spaces.contains("\n  func initialize() {\n"));
        assert!(two_spaces.contains("\n    return 1;\n"));

        let tabs = format_source(source, 4, false);

        assert!(tabs.contains("\n\tfunc initialize() {\n"));
        assert!(tabs.contains("\n\t\treturn 1;\n"));
    }

    #[test]
    fn idempotent_on_already_formatted_source() {
        let source = "func add(a, b) {\n    return a + b;\n}\n";

        let formatted = format_source(source, 4, true);

        assert_eq!(formatted, source);
    }
}
