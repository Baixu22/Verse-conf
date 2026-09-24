# verseconf-lsp

[简体中文](README.md) | **English**

The VerseConf language server (Language Server Protocol), providing live diagnostics,
completion and navigation for `.vcf` files in your editor.

## Install

```bash
cargo install verseconf-lsp
```

The editor extension looks for this executable under `server/bin/<platform>-<arch>/`.
If you are wiring it up yourself, just launch `verseconf-lsp` over stdio.

## Capabilities

| Capability | Notes |
|---|---|
| Diagnostics | Parse errors are published as `Diagnostic`s with line and column, visible live in the editor |
| Incremental sync | Applies `did_change` range increments correctly without corrupting the document |
| Completion | Trigger characters `.` and `=` |
| Hover | Field information; returns `null` when there is nothing to say instead of a boilerplate card |
| Go to definition / find references | Structure across `@include`d files |
| Semantic highlighting | Semantic tokens |

## Editor integration

The VS Code extension ships with the main repository (its `.vsix` is built by CI, which
compiles the language server per platform first). It also handles resolving the server
path per platform and restoring the executable bit on non-Windows platforms.

## See also

- [Main repository](https://github.com/Baixu22/Verse-conf)
- [Core library verseconf-core](https://crates.io/crates/verseconf-core)
- [Tool-protocol server verseconf-mcp](https://crates.io/crates/verseconf-mcp)

## License

MIT OR Apache-2.0
