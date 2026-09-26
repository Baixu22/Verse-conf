# verseconf-mcp

[简体中文](README.md) | **English**

Exposes VerseConf's validation, audit and deterministic editing capabilities to agent
hosts over a **tool protocol** (Model Context Protocol style).

The host does not have to switch configuration formats or understand VerseConf syntax —
it only has to call tools.

## Why tools, and not another format

The model emits structured intent only (which field, what value, why), and deterministic
code resolves the character range and replaces it. That boundary makes edits reviewable by
a human, and makes failures explicitly refused rather than guessed at.

## Install

```bash
cargo install verseconf-mcp
```

## The five tools

| Tool | Purpose |
|---|---|
| `verseconf_validate` | Parse + schema validation, returning errors with line and column |
| `verseconf_audit` | Security audit (wildcard binds, plaintext credentials, …) with stable error codes |
| `verseconf_apply_edit` | Byte-range minimal edit from an edit plan, validated twice before writing |
| `verseconf_edit_range` | Replace an explicit character range (a lower-level entry point) |
| `verseconf_check_write` | Pre-write check: given the original and candidate text, decide whether the change may be written; it does not care how the candidate was produced |

Failures return **structured refusal reasons**, not a paragraph of prose.

## Usage

```bash
verseconf-mcp --list-tools                              # inspect the tool contract offline
verseconf-mcp                                           # stdio session (line-delimited JSON-RPC)
verseconf-mcp --call verseconf_validate '{"source":"port = 8080\n"}'
```

## Host integration

```json
{
  "mcpServers": {
    "verseconf": {
      "command": "verseconf-mcp",
      "args": []
    }
  }
}
```

If a host would rather not depend on a local binary, the same tool implementations are
also compiled to WebAssembly and can be loaded directly by a JS runtime, with no Rust
toolchain required.

> **That distribution path is not published**: the npm package has been abandoned (the
> publishing account is unavailable). The published `verseconf@0.1.0` has no `pkg/` directory
> and neither entry point works; the fixed 0.2.0 is built in-repo and passes its packaging
> checks, but will not be published. Use the local binary form above, or build the wasm
> artifacts from source (see `integrations/verseconf-wasm/js-api`).

## See also

- [Full tool contract and protocol details](https://github.com/Baixu22/Verse-conf/blob/main/docs/MCP.md)
- [Core library verseconf-core](https://crates.io/crates/verseconf-core)
- [Main repository](https://github.com/Baixu22/Verse-conf)

## License

MIT OR Apache-2.0
