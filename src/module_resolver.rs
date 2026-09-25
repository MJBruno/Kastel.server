//! Résolution d'imports du LSP, alignée sur le resolver officiel de Kastel.
//!
//! Le LSP ne réimplémente volontairement pas la sémantique des imports : il
//! réutilise `kastel::module::resolver::ModuleResolver` afin que `std.*`, la
//! résolution locale et le fallback par racine de projet aient exactement le
//! même comportement que le compilateur/VM.

use std::path::{Path, PathBuf};

use kastel::module::resolver::{ImportResolution, ModuleResolver as KastelResolver};

#[derive(Debug, Clone)]
pub struct ModuleResolver {
    inner: KastelResolver,
}

impl ModuleResolver {
    pub fn new(root: Option<PathBuf>) -> Self {
        let root =
            root.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        Self {
            inner: KastelResolver::new(root),
        }
    }


    /// Résout uniquement un module fichier.
    pub fn resolve(&self, current_file: &Path, parts: &[String]) -> Option<PathBuf> {
        self.inner.resolve(current_file, parts).ok()
    }

    /// Résout la forme complète `module` / `module.export` utilisée par
    /// Kastel. Dans le second cas, le fichier du module est retourné avec le
    /// nom exporté séparément.
    pub fn resolve_import(
        &self,
        current_file: &Path,
        parts: &[String],
    ) -> Result<ImportResolution, kastel::error::compile_error::CompileError> {
        self.inner.resolve_import(current_file, parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_project_relative_module() {
        let temp = std::env::temp_dir().join("kastel_lsp_module_test_current");
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();

        let main_file = temp.join("main.ks");
        let module_file = temp.join("math.ks");
        std::fs::write(&main_file, "import math").unwrap();
        std::fs::write(&module_file, "export const PI = 3.14").unwrap();

        let resolver = ModuleResolver::new(Some(temp.clone()));
        let resolved = resolver.resolve(&main_file, &["math".into()]).unwrap();

        assert_eq!(resolved, module_file.canonicalize().unwrap());
        let _ = std::fs::remove_dir_all(temp);
    }
}
