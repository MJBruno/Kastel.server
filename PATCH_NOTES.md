# Correctifs multi-plateforme (Forge 0.2.0)

Fichiers modifiés : `Cargo.toml`, `README.md`, `src/{uri_util,workspace,definition,hover,server,protocol,main}.rs`.

1. **Cargo.toml** : la dépendance `kastel` pointait vers `C:\Users\BRUNO\...` (ne compilait que sur ta machine) → `path = ".."`.
2. **uri_util.rs** : VS Code envoie `file:///c%3A/Users/My%20Project/a.ks`. L'ancien code ne décodait pas `%XX` :
   tous les chemins Windows (`c%3A`) et tous les chemins avec espace/accent échouaient, sur tous les OS.
   Ajout du décodage/encodage, des chemins UNC (`file://server/share`), de `canonical_uri` et `same_uri`.
3. **workspace.rs** : documents indexés par URI canonique (`c:` / `C:`, `%20` / espace ne créent plus de doublons).
4. **definition.rs / hover.rs** : comparaisons `module_uri == current_uri` remplacées par `same_uri`.
5. **main.rs** : lecture des en-têtes tolérante (`\n` seul, casse), les réponses du client (sans `method`) et le JSON invalide
   n'arrêtent plus le serveur, une panique dans l'analyse renvoie une erreur JSON-RPC au lieu de tuer le processus.
6. **protocol.rs / server.rs** : méthode inconnue → vraie erreur JSON-RPC `-32601` (avant : `result` contenant un objet `error`).

Non compilé ici (pas de toolchain Rust ni du crate `kastel` dans mon environnement) : lancer `cargo build` et `cargo test`.
