# VerseConf

[简体中文](README.md) | **English**

> **A deterministic execution layer for agents editing configuration.**
> The model supplies only intent — which field to change and what to change it to.
> Deterministic code performs a byte-range minimal edit, with schema validation and a
> security audit before anything is written. **When it cannot be certain, it refuses
> instead of guessing.**

[![crates.io](https://img.shields.io/crates/v/verseconf-core.svg)](https://crates.io/crates/verseconf-core)
[![CI](https://github.com/Baixu22/Verse-conf/actions/workflows/ci.yml/badge.svg)](https://github.com/Baixu22/Verse-conf/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-green.svg)](LICENSE)

---

## Table of contents

- [1. The Problem](#1-the-problem)
- [2. The Approach](#2-the-approach)
- [3. Measured results](#3-measured-results)
- [4. Installation](#4-installation)
- [5. Quick start](#5-quick-start)
- [6. The configuration language](#6-the-configuration-language)
- [7. CLI reference](#7-cli-reference)
- [8. Wiring into an agent host](#8-wiring-into-an-agent-host)
- [9. Editor support](#9-editor-support)
- [10. Project structure](#10-project-structure)
- [11. Development](#11-development)
- [12. Documentation index](#12-documentation-index)
- [13. Known limitations and roadmap](#13-known-limitations-and-roadmap)

---

## 1. The Problem

The usual way to let a model edit configuration is to hand it the **entire file** and
let it rewrite. The cost of that is measurable:

| Cost | What it looks like |
|------|--------------------|
| Bytes outside the change get rewritten | You wanted one port number changed; the whole file is reformatted |
| Comments and metadata are lost | Trailing comments, `#@` annotations and key order are wiped |
| Edits that should be refused are applied | Ambiguous targets, schema-breaking values and new security risks do not stop it |
| Nothing is auditable | All you get is a new file, with no record of what changed or why |

## 2. The Approach

1. **Accept intent only** — which field, what value, why, and what the preconditions are.
2. **Locate deterministically** — resolve the target value's character range and replace
   only those bytes.
3. **Validate twice before writing** — schema validation plus a security audit; either
   failure refuses the edit.
4. **Fail closed** — a missing, ambiguous or stale target returns a stable error code,
   and the caller must not write any file.

### What we don't do

- We are not trying to be a general-purpose configuration language, and we do not
  compete with Pkl / CUE / KCL on expressiveness.
- We do not pile up language-agnostic bindings for the sake of looking like an ecosystem.
- We do not ask the host to switch configuration formats — the capabilities are exposed
  over a tool protocol instead.

## 3. Measured results

### Edit fidelity (public benchmark)

A fixed corpus of 6 documents and 14 tasks. The corpus and the judge live in the
repository and depend on **neither the network nor a model**:

| Strategy | Correct | Collateral damage | Wrong edit | Refusal accuracy | Comments kept |
|----------|---------|-------------------|------------|------------------|---------------|
| **Intent contract + byte-range minimal edit** | **8/8** | **0/8** | **0/14** | **6/6** | 8/8 |
| Rewrite the whole file after changing the value | 0/8 | 8/8 | 3/14 | 3/6 | 8/8 |
| Replace the first line matching the field name | 2/8 | 3/8 | 7/14 | 2/6 | 3/8 |

- **Collateral damage** means the target value was changed correctly, but the prefix and
  suffix outside the changed range are **no longer byte-identical**.
- **Refusal accuracy** requires the expected error code — refusing for the wrong reason
  does not count, otherwise "refuse everything" would score full marks.
- The rewrite strategy uses this repository's own comment-preserving formatter. A real
  model rewrite would also drop comments, so the collateral damage it shows is a **lower
  bound**.

```bash
cargo run -p verseconf-bench --release            # run the benchmark and write results
cargo run -p verseconf-bench --release -- --check # gate mode (used by CI)
```

Methodology and scope limits: [benchmark/README.md](benchmark/README.md).

### Parse performance

One release run on this machine (Windows 10 / AMD64 / rustc 1.98.1, median of 5 rounds
per format):

| Dataset | VerseConf | TOML | JSON | TOML/VCF | JSON/VCF |
|---------|-----------|------|------|----------|----------|
| small (426B) | 9.55μs | 13.51μs | 1.96μs | 1.42x | 0.20x |
| medium (2.5KB) | 78.51μs | 96.99μs | 13.89μs | 1.24x | 0.18x |
| large (24.8KB) | 756.34μs | 950.42μs | 197.13μs | 1.26x | 0.26x |
| xlarge (263KB) | 6.82ms | 8.49ms | 2.46ms | 1.25x | 0.36x |

VerseConf parses roughly **1.24–1.42x faster than TOML**; `serde_json` is faster than all
three, and VerseConf is about 2.8–5.0x slower than it. All three formats carry the same
configuration written equivalently.

```bash
cargo run -p verseconf-compare --bin generate_test_data
cargo run --release -p verseconf-compare --bin benchmark -- --json compare/benchmark_results.json
python3 compare/generate_charts.py
```

The single source for these numbers is `compare/benchmark_results.json`. The chart script
contains no fallback data, and the benchmark exits non-zero rather than reporting zeros
when data is missing or fails to parse. Full report:
[compare/performance_charts.md](compare/performance_charts.md).

## 4. Installation

### Command line

```bash
cargo install verseconf-cli     # the executable is called verseconf
```

> The package is `verseconf-cli` and the command is `verseconf` — the same way the
> `ripgrep` package installs the `rg` command.

### As a library

```bash
cargo add verseconf-core
```

### Tool-protocol server (agent hosts)

```bash
cargo install verseconf-mcp
verseconf-mcp --list-tools      # inspect the tool contract offline
```

### Editor language server

```bash
cargo install verseconf-lsp
```

### From source

```bash
git clone https://github.com/Baixu22/Verse-conf.git
cd Verse-conf
cargo test --workspace          # one command builds and tests everything
```

### Status of the other distribution channels

| Channel | Status |
|---------|--------|
| crates.io (four crates) | ✅ v0.1.0 published; `cargo install verseconf-cli` verified in a clean directory |
| API docs (docs.rs) | ✅ [docs.rs/verseconf-core](https://docs.rs/verseconf-core) |
| VSCode extension `.vsix` | ⚠️ Produced and uploaded by CI as a build artifact; the repository contains no binary. See [9. Editor support](#9-editor-support) |
| npm package `verseconf` | ❌ **The published 0.1.0 does not work** (the tarball has no `pkg/` directory, so both `require` and `import` fail). The fixed 0.2.0 is built and passes packaging checks locally, but has not been published |
| Browser CDN | ❌ Not published. The `pkg-web/` output is in no published package |

## 5. Quick start

```bash
verseconf parse config.vcf                    # parse and print the structure
verseconf validate config.vcf                 # validate, including the schema
verseconf validate config.vcf --strict        # reject undeclared fields
verseconf format config.vcf                   # format (comments preserved, idempotent)
verseconf format config.vcf --ai-canonical    # AI-friendly canonical form
verseconf audit config.vcf                    # security audit
verseconf doc config.vcf                      # generate docs from the schema
```

### A complete example

This configuration passes `parse`, `validate` and `validate --strict`:

```vcf
#@schema {
  version = "1.0"

  app_name {
    type = "string"
    required = true
    llm_hint = "lowercase with hyphens"
  }

  port {
    type = "integer"
    default = 8080
    range = (1024..65535)
  }

  server {
    type = "table"
    host { type = "string" }
    workers { type = "integer" }
  }

  servers {
    type = "array"
  }
}

app_name = "my-service"
port = 8080

server {
  host = "${HOST|127.0.0.1}"   # interpolated only with --env
  workers = 4
}

[[servers]]
name = "primary"
weight = 10

[[servers]]
name = "replica"
weight = 20
```

### Splitting across files

```vcf
@include "database.vcf" merge=deep_merge
```

The merge strategies are `override` (the default), `append`, `merge` and `deep_merge`.
Any other value is a parse error that points at the column, rather than being silently
ignored.

## 6. The configuration language

### Verified working

| Syntax | Notes |
|--------|-------|
| `key = value` | Scalars: string, integer, float, boolean, array, inline table |
| `block { ... }` | Nested table |
| `[[name]]` | Named array table; multiple elements may share a name (targeted by `match`) |
| `# comment` | Line comments, preserved by both formatting and editing |
| `#@key = value` | Trailing metadata, kept alongside the value |
| `#@schema { ... }` | Types, `required`, `default`, `range`, `enum`, `sensitive`, `llm_hint` |
| `@include "f.vcf" merge=...` | File merging, with cycle detection and a depth limit |
| `${VAR\|default}` | Environment interpolation. **Must be inside a quoted string**, and needs `--env` |
| `{{VAR}}` | Template variables, used with `verseconf template render/list/validate/generate` |

Errors carry `file:line:column` plus context, for example:

```
app.vcf:2:3: validation error: value 99999 for field 'port' exceeds maximum 65535
```

### Not implemented

> This section exists because earlier documentation followed the spec and described
> syntax that **cannot be parsed**. The forms below appear in the spec but are **not
> implemented**; copying them will fail immediately.

| Form | Actual behaviour |
|------|------------------|
| Bare expressions such as `${port + 1}` or `${30s + 500ms}` | **Lexical error**: `unexpected character: '$'`. The AST has `BinaryOp` / `UnitValue` and an evaluator, but no syntax can produce them |
| `${ENV:HOSTNAME}` | Lexical error. Only `${VAR\|default}` exists, and only inside quotes |
| `${VAR:default}`, `${VAR or default}` | Not supported, same as above |
| Hot reload | Only a library implementation (`HotReload`); **there is no user-facing entry point** and no CLI subcommand |
| `#@template` / `#@use` template inheritance | Not implemented. Templates are `{{VAR}}` plus the `verseconf template` subcommands |

The full grammar reference is [docs/SPECIFICATION.md](docs/SPECIFICATION.md); for anything
marked unimplemented there, this section is the authority.

## 7. CLI reference

| Command | Purpose |
|---------|---------|
| `verseconf parse <file>` | Parse and print the structure; `--no-include` disables `@include` expansion |
| `verseconf validate <file>` | Validate including schema; `--strict` rejects undeclared fields; `--fix --write` applies safe fixes |
| `verseconf format <file>` | Format; `-o` writes to a file; `--ai-canonical` for canonical form; `--include` merges includes; `--env` interpolates environment variables |
| `verseconf audit <file>` | Security audit (wildcard binds, plaintext credentials, …) with stable rule codes |
| `verseconf doc <file>` | Generate documentation from the schema |
| `verseconf diff <a> <b>` | Compare two configurations |
| `verseconf env <subcommand>` | Environment management |
| `verseconf template <subcommand>` | `render` / `list` / `validate` / `generate` |
| `verseconf version <subcommand>` | Version management |

Two properties worth calling out:

- `format` is **idempotent** and preserves comments and `#@` metadata;
- `validate --fix` makes **zero byte-level changes** to an already-valid file.

## 8. Wiring into an agent host

`verseconf-mcp` exposes four capabilities as tools a host can discover and call:

| Tool | Purpose |
|------|---------|
| `verseconf_validate` | Parse + schema validation, returning errors with line and column |
| `verseconf_audit` | Security audit with stable error codes |
| `verseconf_apply_edit` | Byte-range minimal edit from an edit plan, validated twice before writing |
| `verseconf_edit_range` | Replace an explicit character range (a lower-level entry point) |

Failures return **structured refusal reasons**, not a paragraph of prose.

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

For zero-install hosts, the same tool implementations are compiled to WebAssembly and can
be loaded by a JS runtime, so the host needs no Rust toolchain. **That npm package is
currently unusable**, however — see the status table in
[4. Installation](#4-installation).

The full tool contract is in [docs/MCP.md](docs/MCP.md).

## 9. Editor support

The VSCode extension provides `.vcf` syntax highlighting and, through the language server,
diagnostics, completion, hover, go-to-definition, find-references and semantic
highlighting. The extension bundle carries the language server for each platform
(`win32-x64` / `linux-x64` / `darwin-x64` / `darwin-arm64`).

The bundle is produced by
[.github/workflows/extension.yml](.github/workflows/extension.yml): a matrix builds the
four language servers, they are aggregated, packaged into `verseconf-<version>.vsix` and
uploaded as a build artifact. **The repository contains no `.vsix` binary**; download it
from the pipeline artifacts and install it:

```bash
code --install-extension verseconf-<version>.vsix
```

Local development, packaging and verification:
[extensions/verseconf-vscode/README.md](extensions/verseconf-vscode/README.md).

> **Known placeholders**: the `verseconf.validate` command only shows a toast and
> `verseconf.schema.generate` says "coming soon". `verseconf.format` forwards to
> `editor.action.formatDocument`, but the language server advertises no
> `documentFormattingProvider`, so that command currently does nothing. Live diagnostics
> and completion are provided by the language server as normal.

## 10. Project structure

```
Verse-conf/
├── crates/
│   ├── verseconf-core/     # Core library: lexer / parser / AST / schema / audit / edit engine
│   ├── verseconf-cli/      # Command line (the `verseconf` executable)
│   ├── verseconf-mcp/      # Tool-protocol server
│   ├── verseconf-lsp/      # Language server
│   ├── verseconf-bench/    # Edit fidelity benchmark (not published)
│   └── verseconf-test/     # Integration and performance tests (not published)
├── benchmark/              # Corpus, tasks and methodology for the public benchmark
├── compare/                # Parse performance comparison (not published)
├── docs/                   # Spec, tutorial, tool-protocol documentation
├── examples/               # Example configurations (including common/ environments/ features/)
├── extensions/             # VSCode extension
├── integrations/           # WebAssembly and JavaScript package
└── .github/workflows/      # CI and extension packaging pipelines
```

A few places inside the core library worth knowing:

| Module | Path | Responsibility |
|--------|------|----------------|
| Lexer | `crates/verseconf-core/src/lexer/` | Tokenization, including CRLF and lone-CR handling |
| Parser | `crates/verseconf-core/src/parser/` | Syntax analysis, including `@include` and merge strategies |
| AST | `crates/verseconf-core/src/ast/` | Syntax tree and source spans |
| Edit | `crates/verseconf-core/src/edit/` | **The edit contract and byte-range minimal edits** |
| Semantic | `crates/verseconf-core/src/semantic/` | Validation, schema, environment interpolation |
| Engine | `crates/verseconf-core/src/engine/` | Audit, merging, formatting, caching, templates |

## 11. Development

```bash
cargo test --workspace                                       # all tests
cargo fmt --all -- --check                                   # formatting gate
cargo clippy --workspace --all-targets -- -D warnings        # lint gate
cargo bench                                                  # performance benchmarks
cargo run -p verseconf-bench --release -- --check             # edit fidelity gate
```

CI has 8 **blocking** jobs: `build-and-test`, `format`, `clippy`, `examples`,
`distribution`, `fidelity-benchmark`, `performance-benchmark` and `wasm-distribution`,
plus 5 jobs in the extension pipeline.

### Publishing

In dependency order — `verseconf-core` must go first, since the others depend on it being
on the registry:

```bash
cargo publish -p verseconf-core
cargo publish -p verseconf-cli
cargo publish -p verseconf-mcp
cargo publish -p verseconf-lsp
```

`cargo package --workspace` checks packaging contents and manifests without publishing.
Before a release, run `cargo publish --dry-run -p verseconf-core`.

### Keeping benchmark data honest

Performance numbers **may only come from a real run**. `compare/generate_charts.py`
contains no fallback data and fails outright if it cannot read
`benchmark_results.json`; CI's `performance-benchmark` job moves the dataset away and
asserts that the benchmark fails rather than reporting zeros.

## 12. Documentation index

| Document | Contents |
|----------|----------|
| [docs/SPECIFICATION.md](docs/SPECIFICATION.md) | Language specification (note the unimplemented markers) |
| [docs/TUTORIAL.md](docs/TUTORIAL.md) | Getting-started tutorial |
| [docs/MCP.md](docs/MCP.md) | Tool-protocol contract and host integration |
| [benchmark/README.md](benchmark/README.md) | Methodology and scope limits of the edit fidelity benchmark |
| [compare/performance_charts.md](compare/performance_charts.md) | Parse performance report (generated from the benchmark) |
| [crates/verseconf-core/README.md](crates/verseconf-core/README.md) | Core API and the minimal-edit example |
| [crates/verseconf-cli/README.md](crates/verseconf-cli/README.md) | Command-line usage |
| [crates/verseconf-mcp/README.md](crates/verseconf-mcp/README.md) | Tool-protocol server |
| [crates/verseconf-lsp/README.md](crates/verseconf-lsp/README.md) | Language server |
| [extensions/verseconf-vscode/README.md](extensions/verseconf-vscode/README.md) | Extension development and packaging |
| [examples/](examples/) | Runnable example configurations |

## 13. Known limitations and roadmap

**Current limitations**

1. The edit fidelity benchmark covers only `set` and the refusal path; the minimal-edit
   property of `insert` / `delete` is covered only by unit tests.
2. The benchmark uses deterministic stand-in strategies and no real model — it measures
   the inherent cost of an approach, not a particular model's score.
3. The language server provides no document formatting, so the extension's format command
   has no effect.
4. Distribution is still being closed out: no usable npm release and no published browser
   CDN build.

**Roadmap**

- Bring `insert` / `delete` and a larger corpus into the benchmark
- Publish the fixed npm package so the zero-install path holds for real
- Add document formatting to the language server

## License

MIT OR Apache-2.0 — see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
