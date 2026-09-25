# Kastel LSP

Language Server Protocol implementation for the current Kastel language.
It is designed for VS Code clients and follows the same interaction model users
expect from TypeScript: completion is context-aware, symbols are resolved from
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

From the Kastel repository root:

```text
cargo build --manifest-path LSP/Cargo.toml --release
```

The executable is produced by Cargo under `LSP/target/release/kastel-lsp`.

## VS Code client

Point the language client at the built `kastel-lsp` executable. No Kastel compiler
or source-code changes outside the LSP package are required by this update.
