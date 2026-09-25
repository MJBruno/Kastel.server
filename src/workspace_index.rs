//! Scan récursif du workspace pour trouver tous les fichiers Kastel.
//!
//! Appelé une fois au moment du `initialize` pour peupler le
//! `Workspace` avec tous les `.ks` du projet. Cela débloque les
//! fonctionnalités cross-fichiers (definition, references, hover...)
//! sans avoir à ouvrir chaque fichier dans l'éditeur.

use std::path::{Path, PathBuf};

/// Dossiers ignorés lors du scan (chemins absolus ou relatifs
/// au répertoire scanné, comparés par nom de dossier).
const IGNORED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    "build",
    "dist",
    "out",
];

/// Plafond de sécurité : évite de bloquer `initialize` sur un
/// dossier pathologique (ex. `C:\` par erreur).
const MAX_FILES: usize = 10_000;

/// Retourne tous les fichiers `.ks` trouvés sous `root`.
///
/// - suit les sous-répertoires récursivement ;
/// - ignore les dossiers cachés (`.git`, `.vscode`, ...) et ceux
///   de `IGNORED_DIRS` ;
/// - renvoie une liste vide si `root` n'existe pas ou n'est pas
///   lisible.
pub fn scan(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk(root, &mut files);
    files
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    if files.len() >= MAX_FILES {
        return;
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        if files.len() >= MAX_FILES {
            return;
        }

        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        let path = entry.path();

        if file_type.is_dir() {
            if should_skip_dir(&path) {
                continue;
            }
            walk(&path, files);
        } else if file_type.is_file() && is_kastel_file(&path) {
            files.push(path);
        }
    }
}

/// Ignore les dossiers cachés (`name.starts_with('.')`) et ceux
/// présents dans `IGNORED_DIRS`.
fn should_skip_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return true;
    };

    if name.starts_with('.') {
        return true;
    }

    IGNORED_DIRS.contains(&name)
}

fn is_kastel_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("ks")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);

        let _ = std::fs::remove_dir_all(&dir);

        std::fs::create_dir_all(&dir).expect("failed to create temp dir");

        dir
    }

    fn cleanup(dir: &Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn scan_finds_kastel_files_at_root() {
        let root = fresh_dir("kastel_scan_root");

        std::fs::write(root.join("main.ks"), "").unwrap();

        std::fs::write(root.join("other.ks"), "").unwrap();

        std::fs::write(root.join("README.md"), "").unwrap();

        let mut files = scan(&root);

        files.sort();

        let names: Vec<_> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert_eq!(names, vec!["main.ks", "other.ks"]);

        cleanup(&root);
    }

    #[test]
    fn scan_recurses_into_subdirs() {
        let root = fresh_dir("kastel_scan_recursive");

        std::fs::create_dir_all(root.join("math")).unwrap();

        std::fs::write(root.join("main.ks"), "").unwrap();

        std::fs::write(root.join("math").join("xx.ks"), "").unwrap();

        let files = scan(&root);

        assert_eq!(files.len(), 2);

        cleanup(&root);
    }

    #[test]
    fn scan_skips_ignored_dirs() {
        let root = fresh_dir("kastel_scan_ignored");

        std::fs::create_dir_all(root.join("target")).unwrap();

        std::fs::create_dir_all(root.join(".git")).unwrap();

        std::fs::write(root.join("main.ks"), "").unwrap();

        std::fs::write(root.join("target").join("skip.ks"), "").unwrap();

        std::fs::write(root.join(".git").join("hidden.ks"), "").unwrap();

        let files = scan(&root);

        assert_eq!(files.len(), 1);

        assert!(
            files[0].ends_with("main.ks"),
            "unexpected file: {:?}",
            files[0]
        );

        cleanup(&root);
    }

    #[test]
    fn scan_on_missing_dir_returns_empty() {
        let path = std::env::temp_dir().join("kastel_scan_missing_xyz");

        let _ = std::fs::remove_dir_all(&path);

        assert!(scan(&path).is_empty());
    }

    #[test]
    fn scan_ignores_non_ks_extensions() {
        let root = fresh_dir("kastel_scan_extensions");

        std::fs::write(root.join("main.ks"), "").unwrap();

        std::fs::write(root.join("main.txt"), "").unwrap();

        std::fs::write(root.join("main.ks.bak"), "").unwrap();

        let files = scan(&root);

        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("main.ks"));

        cleanup(&root);
    }
}
