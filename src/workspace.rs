use std::collections::HashMap;
use std::path::{Path, PathBuf};

use kastel::frontend::ast::Statement;

use crate::analyzer::{Diagnostic, analyze};
use crate::class_index::ClassIndex;
use crate::symbols::SymbolIndex;
use crate::type_info::TypeInfo;

#[derive(Debug)]
pub struct WorkspaceDocument {
    pub text: String,
    pub version: i64,
    pub symbols: SymbolIndex,
    pub classes: ClassIndex,
    pub statements: Vec<Statement>,
    pub types: TypeInfo,
    pub diagnostics: Vec<Diagnostic>,
}

impl WorkspaceDocument {
    pub fn new(version: i64, text: String) -> Self {
        let analysis = analyze(&text);
        let mut symbols = analysis.symbols.clone();
        if symbols.is_empty() && !analysis.statements.is_empty() {
            symbols.rebuild(&text, &analysis.statements);
        }

        let mut classes = ClassIndex::new();
        classes.rebuild(&analysis.statements);

        let types = TypeInfo::build(&analysis.statements);

        Self {
            text,
            version,
            symbols,
            classes,
            statements: analysis.statements,
            types,
            diagnostics: analysis.diagnostics,
        }
    }

    pub fn update(&mut self, version: i64, text: String) {
        let analysis = analyze(&text);

        self.version = version;
        self.text = text;
        self.symbols = analysis.symbols.clone();
        self.statements = analysis.statements;
        self.types = TypeInfo::build(&self.statements);
        self.classes.rebuild(&self.statements);
        self.diagnostics = analysis.diagnostics;
    }
}

#[derive(Debug, Default)]
pub struct Workspace {
    documents: HashMap<String, WorkspaceDocument>,
    root: Option<PathBuf>,
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            root: None,
        }
    }

    pub fn set_root(&mut self, root: PathBuf) {
        self.root = Some(root);
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn open(&mut self, uri: String, version: i64, text: String) {
        self.documents
            .insert(uri, WorkspaceDocument::new(version, text));
    }

    pub fn open_file(&mut self, uri: String, path: &Path) -> std::io::Result<()> {
        let text = std::fs::read_to_string(path)?;
        self.documents.insert(uri, WorkspaceDocument::new(0, text));
        Ok(())
    }

    pub fn update(&mut self, uri: &str, version: i64, text: String) -> bool {
        let Some(document) = self.documents.get_mut(uri) else {
            return false;
        };
        document.update(version, text);
        true
    }

    pub fn close(&mut self, uri: &str) -> Option<WorkspaceDocument> {
        self.documents.remove(uri)
    }

    pub fn get(&self, uri: &str) -> Option<&WorkspaceDocument> {
        self.documents.get(uri)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &WorkspaceDocument)> {
        self.documents.iter()
    }

    pub fn len(&self) -> usize {
        self.documents.len()
    }
}
