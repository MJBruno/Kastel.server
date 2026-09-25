//! Conversion entre URIs LSP `file://` et chemins du système.
//!
//! Ces fonctions sont pures : pas de `canonicalize`, pas d'accès disque.
//! L'appelant décide s'il veut canonicaliser avant/après.

use std::path::{Path, PathBuf};

/// Convertit une URI `file://` en chemin local.
///
/// - Windows : `file:///C:/Users/foo/main.ks` → `C:\Users\foo\main.ks`
/// - Windows : `file:////server/share/x`     → `\\server\share\x`
/// - Unix    : `file:///home/user/main.ks`   → `/home/user/main.ks`
///
/// Renvoie `None` si l'URI n'est pas de type `file:///`.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file:///")?;

    #[cfg(windows)]
    {
        let mut path = path.replace('/', "\\");

        // Cas "C:\..." (drive letter)
        if path.len() >= 2 && path.as_bytes()[0].is_ascii_alphabetic() && path.as_bytes()[1] == b':'
        {
            return Some(PathBuf::from(path));
        }

        // Cas UNC "\\server\share\..." :
        // l'URI était "file:////server/share/..."
        // → après strip on a "/server/share/..."
        // → après replace on a "\server\share\..."
        // → il faut ajouter un "\" pour obtenir "\\server\share\..."
        if path.starts_with('\\') && !path.starts_with("\\\\") {
            path = format!("\\{}", path);
        }

        Some(PathBuf::from(path))
    }

    #[cfg(not(windows))]
    {
        Some(PathBuf::from(format!("/{}", path)))
    }
}

/// Convertit un chemin local en URI `file://`.
///
/// - Windows : `C:\Users\foo\main.ks` → `file:///C:/Users/foo/main.ks`
/// - Unix    : `/home/user/main.ks`   → `file:///home/user/main.ks`
///
/// Le préfixe étendu Windows `\\?\` est retiré s'il est présent.
pub fn path_to_uri(path: &Path) -> String {
    #[cfg(windows)]
    {
        let mut path = path.to_string_lossy().to_string();

        if let Some(stripped) = path.strip_prefix(r"\\?\") {
            path = stripped.to_string();
        }

        path = path.replace('\\', "/");

        format!("file:///{}", path)
    }

    #[cfg(not(windows))]
    {
        // Le path Unix commence déjà par '/', donc
        // "file://" + "/home/..." = "file:///home/...".
        format!("file://{}", path.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_to_path_rejects_non_file_uri() {
        assert!(uri_to_path("http://example.com").is_none());
        assert!(uri_to_path("untitled:Untitled-1").is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn uri_to_path_handles_posix_uri() {
        let path = uri_to_path("file:///home/user/main.ks").unwrap();

        assert_eq!(path, PathBuf::from("/home/user/main.ks"));
    }

    #[cfg(not(windows))]
    #[test]
    fn path_to_uri_handles_posix_path() {
        let uri = path_to_uri(Path::new("/home/user/main.ks"));

        assert_eq!(uri, "file:///home/user/main.ks");
    }

    #[cfg(not(windows))]
    #[test]
    fn posix_round_trip_is_stable() {
        let original = PathBuf::from("/home/user/project/main.ks");

        let uri = path_to_uri(&original);

        let back = uri_to_path(&uri).unwrap();

        assert_eq!(back, original);
    }

    #[cfg(windows)]
    #[test]
    fn windows_uri_to_path_restores_backslashes() {
        let path = uri_to_path("file:///C:/Users/foo/main.ks").unwrap();

        assert_eq!(path, PathBuf::from(r"C:\Users\foo\main.ks"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_to_uri_produces_three_slashes() {
        let uri = path_to_uri(Path::new(r"C:\Users\foo\main.ks"));

        assert_eq!(uri, "file:///C:/Users/foo/main.ks");
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_to_uri_strips_extended_prefix() {
        let uri = path_to_uri(Path::new(r"\\?\C:\Users\foo\main.ks"));

        assert_eq!(uri, "file:///C:/Users/foo/main.ks");

        assert!(!uri.contains(r"\\?\"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_round_trip_is_stable() {
        let original = PathBuf::from(r"C:\Users\foo\project\main.ks");

        let uri = path_to_uri(&original);

        let back = uri_to_path(&uri).unwrap();

        assert_eq!(back, original);
    }

    #[cfg(windows)]
    #[test]
    fn windows_unc_uri_to_path() {
        // "file:////server/share/file.ks" (4 slashes)
        let path = uri_to_path("file:////server/share/file.ks").unwrap();

        assert_eq!(path, PathBuf::from(r"\\server\share\file.ks"));
    }
}
