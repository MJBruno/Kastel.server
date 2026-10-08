//! Résolution d'imports du LSP, alignée sur le resolver officiel de Kastel.
//!
//! Le LSP ne réimplémente volontairement pas la sémantique des imports : il
//! réutilise `kastel::module::resolver::ModuleResolver` afin que `std.*`, la
//! résolution locale et le fallback par racine de projet aient exactement le
//! même comportement que le compilateur/VM.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use kastel::module::resolver::{ImportResolution, ModuleResolver as KastelResolver};

/// Localise la bibliothèque standard `std/` pour le LSP.
///
/// Le resolver de `kastel` cherche `std` à côté de l'exécutable courant, puis dans
/// `CARGO_MANIFEST_DIR` — un chemin figé à la compilation qui n'existe que sur la
/// machine du développeur. Le LSP est installé ailleurs (extension VS Code), il
/// teste donc, dans l'ordre :
///
///   1. `KASTEL_STD_PATH` (positionnée par l'extension Forge) ;
///   2. `<dossier du LSP>/std`, `<dossier du LSP>/../std`, `<dossier du LSP>/../../std`
///      (ce dernier : `<extension>/server/<os>-<arch>/kastel-lsp` → `<extension>/std`).
///
/// `None` laisse le comportement par défaut de `kastel`. Résultat mis en cache.
fn locate_std_root() -> Option<PathBuf> {
    static STD_ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();

    STD_ROOT
        .get_or_init(|| {
            if let Ok(custom) = std::env::var("KASTEL_STD_PATH") {
                let path = PathBuf::from(custom);

                if path.is_dir() {
                    return Some(path);
                }
            }

            let exe = std::env::current_exe().ok()?;
            let dir = exe.parent()?.to_path_buf();

            [
                dir.join("std"),
                dir.join("..").join("std"),
                dir.join("..").join("..").join("std"),
            ]
            .into_iter()
            .find(|candidate| candidate.is_dir())
        })
        .clone()
}

#[derive(Debug, Clone)]
pub struct ModuleResolver {
    inner: KastelResolver,
}

impl ModuleResolver {
    pub fn new(root: Option<PathBuf>) -> Self {
        let root =
            root.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let inner = KastelResolver::new(root);

        let inner = match locate_std_root() {
            Some(std_root) => inner.with_std_root(std_root),
            None => inner,
        };

        Self { inner }
    }


    /// Résout uniquement un module fichier.
    pub fn resolve(&self, current_file: &Path, parts: &[String]) -> Option<PathBuf> {
        self.inner.resolve(current_file, parts).ok()
    }

    /// Fichier du module désigné par `parts`, y compris pour `import module.export`
    /// (le fichier du module est alors celui qui contient l'export).
    pub fn resolve_file(&self, current_file: &Path, parts: &[String]) -> Option<PathBuf> {
        match self.inner.resolve_import(current_file, parts).ok()? {
            ImportResolution::Module(path) | ImportResolution::Export { module: path, .. } => {
                Some(path)
            }
        }
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
