//! Conversion entre URIs LSP `file://` et chemins du système.
//!
//! Ces fonctions sont pures : pas de `canonicalize`, pas d'accès disque.
//! L'appelant décide s'il veut canonicaliser avant/après.
//!
//! Points importants pour le multi-plateforme :
//!
//! - VS Code envoie des URIs encodées (`file:///c%3A/Users/My%20Project/a.ks`) :
//!   les séquences `%XX` sont décodées (UTF-8) avant conversion en chemin.
//! - `path_to_uri` ré-encode les caractères non sûrs (espaces, accents, `#`, `%`…).
//! - `canonical_uri` donne une forme stable (lettre de lecteur Windows en
//!   minuscule, encodage uniforme) : c'est la clé utilisée par le `Workspace`.

use std::path::{Path, PathBuf};

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Décode les séquences `%XX` d'une URI (octets interprétés en UTF-8).
/// Une séquence invalide est conservée telle quelle.
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push(high * 16 + low);
                i += 3;
                continue;
            }
        }

        out.push(bytes[i]);
        i += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

fn is_path_safe(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b'/'
                | b':'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
        )
}

/// Encode un chemin pour l'inclure dans une URI (espaces, accents, `#`, `%`, `?`…).
pub fn percent_encode_path(input: &str) -> String {
    let mut out = String::with_capacity(input.len());

    for &byte in input.as_bytes() {
        if is_path_safe(byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }

    out
}

/// Convertit une URI `file://` en chemin local.
///
/// - Windows : `file:///C:/Users/foo/main.ks`   → `C:\Users\foo\main.ks`
/// - Windows : `file:///c%3A/Users/foo/a.ks`    → `c:\Users\foo\a.ks` (forme VS Code)
/// - Windows : `file:////server/share/x`        → `\\server\share\x`
/// - Windows : `file://server/share/x`          → `\\server\share\x`
/// - Unix    : `file:///home/user/My%20App/a.ks` → `/home/user/My App/a.ks`
///
/// Renvoie `None` si l'URI n'est pas de type `file://`.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let decoded = percent_decode(rest);

    #[cfg(windows)]
    {
        let slashed = decoded.replace('/', "\\");
        let bytes = slashed.as_bytes();

        // "\C:\Users\..." (issu de "file:///C:/Users/...")
        if bytes.len() >= 3
            && bytes[0] == b'\\'
            && bytes[1].is_ascii_alphabetic()
            && bytes[2] == b':'
        {
            return Some(PathBuf::from(&slashed[1..]));
        }

        // "\\server\share\..." (issu de "file:////server/share/...")
        if slashed.starts_with("\\\\") {
            return Some(PathBuf::from(slashed));
        }

        // "server\share\..." (issu de "file://server/share/...")
        if !slashed.starts_with('\\') {
            return Some(PathBuf::from(format!("\\\\{}", slashed)));
        }

        Some(PathBuf::from(slashed))
    }

    #[cfg(not(windows))]
    {
        let local = decoded
            .strip_prefix("localhost")
            .filter(|remaining| remaining.starts_with('/'))
            .map(str::to_owned)
            .unwrap_or_else(|| decoded.clone());

        if local.starts_with('/') {
            Some(PathBuf::from(local))
        } else {
            None
        }
    }
}

/// Convertit un chemin local en URI `file://`.
///
/// - Windows : `C:\Users\foo\main.ks`   → `file:///C:/Users/foo/main.ks`
/// - Windows : `\\server\share\x.ks`    → `file://server/share/x.ks`
/// - Unix    : `/home/user/main.ks`     → `file:///home/user/main.ks`
///
/// Les préfixes étendus Windows (`\\?\` et `\\?\UNC\`) sont retirés.
/// Les caractères non sûrs sont encodés en `%XX`.
pub fn path_to_uri(path: &Path) -> String {
    #[cfg(windows)]
    {
        let mut text = path.to_string_lossy().to_string();

        if let Some(stripped) = text.strip_prefix(r"\\?\UNC\") {
            text = format!(r"\\{}", stripped);
        } else if let Some(stripped) = text.strip_prefix(r"\\?\") {
            text = stripped.to_string();
        }

        let text = text.replace('\\', "/");

        if let Some(unc) = text.strip_prefix("//") {
            return format!("file://{}", percent_encode_path(unc));
        }

        format!("file:///{}", percent_encode_path(&text))
    }

    #[cfg(not(windows))]
    {
        // Le path Unix commence déjà par '/', donc
        // "file://" + "/home/..." = "file:///home/...".
        format!("file://{}", percent_encode_path(&path.to_string_lossy()))
    }
}

#[cfg(windows)]
fn lowercase_drive(uri: String) -> String {
    let Some(rest) = uri.strip_prefix("file:///") else {
        return uri;
    };

    let bytes = rest.as_bytes();

    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let mut out = String::with_capacity(uri.len());
        out.push_str("file:///");
        out.push(bytes[0].to_ascii_lowercase() as char);
        out.push_str(&rest[1..]);
        return out;
    }

    uri
}

#[cfg(not(windows))]
fn lowercase_drive(uri: String) -> String {
    uri
}

/// Forme canonique d'une URI : même fichier ⇒ même chaîne, quelle que soit la
/// manière dont l'éditeur ou le LSP l'a écrite (`c%3A` / `C:`, `%20` / espace…).
/// Les URIs qui ne sont pas des `file://` sont renvoyées telles quelles.
pub fn canonical_uri(uri: &str) -> String {
    match uri_to_path(uri) {
        Some(path) => lowercase_drive(path_to_uri(&path)),
        None => uri.to_string(),
    }
}

/// Vrai si les deux URIs désignent le même fichier.
pub fn same_uri(left: &str, right: &str) -> bool {
    left == right || canonical_uri(left) == canonical_uri(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_to_path_rejects_non_file_uri() {
        assert!(uri_to_path("http://example.com").is_none());
        assert!(uri_to_path("untitled:Untitled-1").is_none());
    }

    #[test]
    fn percent_decode_handles_escapes() {
        assert_eq!(percent_decode("My%20Project"), "My Project");
        assert_eq!(percent_decode("c%3A/x"), "c:/x");
        assert_eq!(percent_decode("caf%C3%A9"), "café");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn percent_encode_escapes_unsafe_characters() {
        assert_eq!(percent_encode_path("/a b/é#.ks"), "/a%20b/%C3%A9%23.ks");
        assert_eq!(percent_encode_path("/home/user/main.ks"), "/home/user/main.ks");
    }

    #[test]
    fn same_uri_ignores_encoding_differences() {
        assert!(same_uri("file:///home/a%2Db.ks", "file:///home/a-b.ks"));
        assert!(!same_uri("file:///home/a.ks", "file:///home/b.ks"));
    }

    #[cfg(not(windows))]
    #[test]
    fn uri_to_path_handles_posix_uri() {
        let path = uri_to_path("file:///home/user/main.ks").unwrap();

        assert_eq!(path, PathBuf::from("/home/user/main.ks"));
    }

    #[cfg(not(windows))]
    #[test]
    fn uri_to_path_decodes_spaces() {
        let path = uri_to_path("file:///home/me/My%20Project/main.ks").unwrap();

        assert_eq!(path, PathBuf::from("/home/me/My Project/main.ks"));
    }

    #[cfg(not(windows))]
    #[test]
    fn path_to_uri_handles_posix_path() {
        let uri = path_to_uri(Path::new("/home/user/main.ks"));

        assert_eq!(uri, "file:///home/user/main.ks");
    }

    #[cfg(not(windows))]
    #[test]
    fn path_to_uri_encodes_spaces() {
        let uri = path_to_uri(Path::new("/home/me/My Project/main.ks"));

        assert_eq!(uri, "file:///home/me/My%20Project/main.ks");
    }

    #[cfg(not(windows))]
    #[test]
    fn posix_round_trip_is_stable() {
        let original = PathBuf::from("/home/user/my project/main.ks");

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
    fn windows_vscode_uri_is_decoded() {
        let path = uri_to_path("file:///c%3A/Users/My%20Project/main.ks").unwrap();

        assert_eq!(path, PathBuf::from(r"c:\Users\My Project\main.ks"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_drive_case_is_ignored_by_same_uri() {
        assert!(same_uri(
            "file:///c%3A/Users/foo/main.ks",
            "file:///C:/Users/foo/main.ks"
        ));
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
        let original = PathBuf::from(r"C:\Users\foo\my project\main.ks");

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

        // forme VS Code : "file://server/share/file.ks"
        let path = uri_to_path("file://server/share/file.ks").unwrap();

        assert_eq!(path, PathBuf::from(r"\\server\share\file.ks"));
    }
}
