use kastel::error::lex_error::LexerError;
use kastel::error::parse_error::ParserError;
use kastel::frontend::ast::Statement;
use kastel::frontend::lexer::lexer::Lexer;
use kastel::frontend::parser::Parser;

use crate::symbols::SymbolIndex;

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub line: usize,
    pub column: usize,
    /// 1 = Error, 2 = Warning, 3 = Information, 4 = Hint (LSP).
    pub severity: u32,
    /// Code court (ex. "lexer", "parser", "undefined-identifier").
    pub code: Option<String>,
}

impl Diagnostic {
    pub fn error(message: String, line: usize, column: usize, code: &str) -> Self {
        Self {
            message,
            line,
            column,
            severity: 1,
            code: Some(code.to_string()),
        }
    }

    #[allow(dead_code)]
    pub fn warning(message: String, line: usize, column: usize, code: &str) -> Self {
        Self {
            message,
            line,
            column,
            severity: 2,
            code: Some(code.to_string()),
        }
    }
}

pub struct AnalysisResult {
    pub statements: Vec<Statement>,
    pub symbols: SymbolIndex,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn analyze(source: &str) -> AnalysisResult {
    let mut lexer = Lexer::new(source.to_owned());

    let tokens = match lexer.scan_token() {
        Ok(tokens) => tokens,
        Err(errors) => {
            let mut symbols = SymbolIndex::new();
            symbols.rebuild_fallback(source);
            return AnalysisResult {
                statements: Vec::new(),
                symbols,
                diagnostics: errors.into_iter().map(lexer_diagnostic).collect(),
            };
        }
    };

    let mut parser = Parser::new(tokens);

    let statements = match parser.parse() {
        Ok(statements) => statements,
        Err(errors) => {
            let mut symbols = SymbolIndex::new();
            symbols.rebuild_fallback(source);
            return AnalysisResult {
                statements: Vec::new(),
                symbols,
                diagnostics: errors.into_iter().map(parser_diagnostic).collect(),
            };
        }
    };

    /*
     * Construction de l'index des symboles en une seule passe.
     * Réutilisé par WorkspaceDocument et (plus tard) par
     * l'analyse sémantique.
     */
    let mut symbols = SymbolIndex::new();
    symbols.rebuild(source, &statements);

    AnalysisResult {
        statements,
        symbols,
        diagnostics: Vec::new(),
    }
}

fn lexer_diagnostic(error: LexerError) -> Diagnostic {
    Diagnostic::error(error.message, error.line, error.column, "lexer")
}

fn parser_diagnostic(error: ParserError) -> Diagnostic {
    Diagnostic::error(error.message, error.line, error.column, "parser")
}
