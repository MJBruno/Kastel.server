# KASTEL LSP

Language Server Protocol implementation for the current Kastel language.
It is designed for VS Code clients and follows the same interaction model users: completion is context-aware, symbols are resolved from
the workspace, signatures and hover information are derived from the language
type model, and edits remain useful while the source is incomplete during typing.

## Current Kastel support

The LSP is aligned with the current compiler/parser/runtime APIs:

- `func`, `let`, `const`, `class`, `interface`, `type`, `import`, `from`, `export`
- `initialize()` constructors
- `List<T>`, `Dict<K, V>`, `Tuple<...>`, `Set<T>`, `Range`
- union types such as `int | float`
- dynamic typing through `dynamic` / `any`
- class inheritance and visibility (`public` / `private`)
- module-qualified imports such as `dog.Dog` and `std.math`
- exported module members
- the current container API (`size`, `add`, `remove_at`, `copy`, etc.)

The LSP deliberately uses Kastel's own `ModuleResolver` and `Type` model rather
than maintaining a second incompatible import/type system.

## VS Code language features

Implemented through standard LSP requests:

- diagnostics from lexer/parser plus lightweight semantic undefined-name checks
- completion with identifier-prefix and `receiver.member` context
- module/import completion
- class/interface members including inherited members
- hover for functions, variables, types, classes, interfaces, modules and members
- go to definition for local, imported, module and class/interface members
- find references
- document highlights
- rename with `prepareRename`
- document symbols
- workspace symbols
- signature help with overload-like signatures and active parameter tracking
- document formatting

The workspace is indexed from the LSP `rootUri`, `workspaceFolders` or legacy
`rootPath`, so imported modules can be resolved before they are opened in the
editor.

## Build

Le crate dépend de `kastel` par chemin **relatif** (`path = ".."` dans `Cargo.toml`) :
le LSP doit donc se trouver dans `LSP/` à la racine du dépôt Kastel (sinon adapter ce chemin).

```text
cd LSP
cargo build --release
```

L'exécutable est produit dans `target/release/kastel-lsp` (`kastel-lsp.exe` sous Windows).
Même code source sur Windows, macOS et Linux ; `cargo test` exécute les tests d'URI propres à chaque OS.

## VS Code client

Use the Forge VS Code extension (it bundles or auto-detects the `kastel-lsp` executable for each OS), or point any LSP client at the built `kastel-lsp` executable (stdio). No Kastel compiler
or source-code changes outside the LSP package are required by this update.
