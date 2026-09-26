# VerseConf

[简体中文](README.md) | **English**

> **A deterministic execution layer for agents editing configuration.**
> The model supplies only intent — which field to change and what to change it to.
> Deterministic code performs a byte-range minimal edit, with schema validation and a
> security audit before anything is written. **When it cannot be certain, it refuses
> instead of guessing.**
>
> ⚠️ **The confirmatory replication measured no net benefit**: the hard gates failed, and the
> project has been closed out as a **small reliable configuration-editing library** with no
> further platform narrative. See [13. Known limitations](#13-known-limitations-and-roadmap),
> item 5.

[![crates.io](https://img.shields.io/crates/v/verseconf-core.svg)](https://crates.io/crates/verseconf-core)
[![CI](https://github.com/Baixu22/Verse-conf/actions/workflows/ci.yml/badge.svg)](https://github.com/Baixu22/Verse-conf/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-green.svg)](LICENSE)

---

## Table of contents

- [1. The problem](#1-the-problem)
- [2. The intent execution protocol](#2-the-intent-execution-protocol)
- [3. Measured results: edit fidelity](#3-measured-results-edit-fidelity)
- [4. Integration](#4-integration)
- [5. Installation](#5-installation)
- [6. Quick start](#6-quick-start)
- [7. CLI reference](#7-cli-reference)
- [8. Configuration language reference](#8-configuration-language-reference)
- [9. Editor support](#9-editor-support)
- [10. Project structure](#10-project-structure)
- [11. Development](#11-development)
- [12. Documentation index](#12-documentation-index)
- [13. Known limitations and roadmap](#13-known-limitations-and-roadmap)
- [Appendix A: parse performance](#appendix-a-parse-performance)

---

## 1. The problem

The usual way to let a model edit configuration is to hand it the **entire file** and
let it rewrite. The cost of that is measurable:

| Cost | What it looks like |
|------|--------------------|
| Bytes outside the change get rewritten | You wanted one port number changed; the whole file is reformatted |
| Comments and metadata are lost | Trailing comments, `#@` annotations and key order are wiped |
| Edits that should be refused are applied | Ambiguous targets, schema-breaking values and new security risks do not stop it |
| Nothing is auditable | All you get is a new file, with no record of what changed or why |

This is not a prompting problem, it is an **interface problem**: as long as the interface
takes a new file as input and returns a new file as output, none of those four costs can be
ruled out at the protocol level. VerseConf replaces the interface with **intent**.

## 2. The intent execution protocol

Every edit takes four steps. The model takes part in the first one only; the other three are
deterministic code:

| Step | Who | Output |
|------|-----|--------|
| 1. Intent contract | the model | an edit plan (JSON): which field, what value, why, and what the preconditions are |
| 2. Resolution | deterministic code | the character range of the target value in the source |
| 3. Minimal edit | deterministic code | only the bytes in that range are replaced; bytes outside it do not change |
| 4. Double validation | deterministic code | the result still parses, still passes structural/schema validation, and introduces no new high-severity security finding; any failure refuses the edit |

The contract is public: [`crates/verseconf-core/schemas/edit-plan.schema.json`](crates/verseconf-core/schemas/edit-plan.schema.json).
Four properties matter:

- **Lists are addressed by name, not by index**: `"path": [{ "key": "servers", "match": { "name": "primary" } }, "ip"]`.
  Matching more than one element is refused rather than resolved by picking the first.
- **`expect` is a precondition**: if the current value does not match, the edit is refused
  instead of being applied on top of a stale assumption.
- **Only the target value's byte range changes**: comments, `#@` metadata, key order, blank
  lines, indentation style and CRLF all survive untouched.
- **Fail closed**: a refusal carries **no `source` field at all**, so the caller cannot write a
  half-finished file even by accident.

### 2.1 A minimal edit you can reproduce

The model only has to produce this:

```json
{ "version": "1.0",
  "edits": [{ "op": "set", "path": ["server", "host"], "value": "0.0.0.0",
              "reason": "listen on every interface" }] }
```

Input:

```vcf
server {
  host = "127.0.0.1"   # bind address
  port = 8080          #@ range(1024..65535)
}
```

The `source` the tool returns:

```vcf
server {
  host = "0.0.0.0"   # bind address
  port = 8080          #@ range(1024..65535)
}
```

Checked at the byte level: the replaced range is exactly `"127.0.0.1"` (source bytes 18..29),
the **prefix before it and the suffix after it are byte-identical**, the trailing comment and
the `#@` metadata are preserved verbatim, and the length delta is exactly `-2`
(`"0.0.0.0"` is two bytes shorter than `"127.0.0.1"`). CRLF documents behave the same way:
line endings stay `\r\n` and are never normalised to LF.

Reproduce locally (no model, no network):

```rust
use verseconf_core::{apply_edit_plan, EditPlan};
let src = "server {\n  host = \"127.0.0.1\"   # bind address\n  port = 8080          #@ range(1024..65535)\n}\n";
let plan: EditPlan = serde_json::from_str(r#"{"version":"1.0","edits":[{"op":"set","path":["server","host"],"value":"0.0.0.0"}]}"#)?;
let out = apply_edit_plan(src, &plan)?;   // returns Err on refusal, never a half-finished result
assert_eq!(out.source, "server {\n  host = \"0.0.0.0\"   # bind address\n  port = 8080          #@ range(1024..65535)\n}\n");
```

It runs after `cargo add verseconf-core`. The same code exists as
[`crates/verseconf-core/examples/deterministic_edit.rs`](crates/verseconf-core/examples/deterministic_edit.rs)
and is compiled by `cargo test --workspace`, so this example cannot drift away from the
implementation. A command-line smoke test (offline, after `cargo install verseconf-mcp`):

```bash
verseconf-mcp --call verseconf_apply_edit \
  '{"source":"server {\n  host = \"127.0.0.1\"   # bind address\n}\n","plan":{"version":"1.0","edits":[{"op":"set","path":["server","host"],"value":"0.0.0.0"}]}}'
```

The equivalent host-side form is `applyEdit(source, plan)` (the WebAssembly distribution, see
[4. Integration](#4-integration)). The edit tools **return text and never write files**;
persisting the result is the host's decision.

### 2.2 The refusal paths

Fail-closed is not a slogan: the six refusal tasks in the benchmark require a **stable error
code**, and refusing for the wrong reason does not count as a pass (otherwise "refuse
everything" would score full marks). The implementation returns these messages in
Chinese; the codes are the stable contract.

| Error code | Trigger | Measured response |
|------------|---------|-------------------|
| `target_ambiguous` | a named list matches several elements | 目标有歧义：`servers[name="primary"].ip` 命中了 2 个元素，必须唯一命中 |
| `target_not_found` | the path does not exist | 目标不存在：`server.missing` |
| `search_incomplete` | the include graph exceeds the search limit, so uniqueness cannot be proven | 搜索不完整：`port` 的 include 图超过上限 64，只扫描了 64 个文件，无法证明目标唯一 |
| `expectation_mismatch` | the current value differs from `expect` | 前置条件不符：`server.port` 期望 1234，实际 8080 |
| `validation_failed` | the result breaks the schema (including the merged effective configuration) | 改动后校验失败：`type mismatch for field 'port': expected integer, found "not-a-number"` |
| `security_rejected` | the edit adds a new high-severity finding instance | 改动引入新的安全风险：`<result>`（`SEC-005 @ tls.ssl_verify`）；`details.instances` 指出是哪个字段 |
| `invalid_plan` | the edit plan violates the contract | 编辑计划不合法（`details.violations[].code = unsupported_version`） |

A refusal looks like this:

```json
{ "code": "target_not_found",
  "message": "目标不存在：server.missing",
  "details": { "path": "server.missing" } }
```

**There is no `source` field** — the caller could not write a half-finished file even if it
tried. The full code table (including `invalid_arguments` / `parse_failed` /
`unsupported_target`) is in [docs/MCP.md](docs/MCP.md).

### 2.3 What we don't do

- We are not trying to be a general-purpose configuration language, and we do not
  compete with Pkl / CUE / KCL on expressiveness.
- We do not pile up language-agnostic bindings for the sake of looking like an ecosystem.
- We do not ask the host to switch configuration formats — the capabilities are exposed
  over a tool protocol instead.

## 3. Measured results: edit fidelity

A fixed corpus of 8 documents and 27 tasks (8 single `set`, 6 `insert`/`delete`,
3 multi-edit, 4 across `@include`, 6 refusals). The corpus and the judge live in the repository
and depend on **neither the network nor a model**. Corpus fingerprint `162777399290a695`,
judging methodology version `1.2`:

| Strategy | Correct | Collateral damage | Wrong edit | Refusal accuracy | Comments kept |
|----------|---------|-------------------|------------|------------------|---------------|
| **Intent contract + byte-range minimal edit** | **21/21** | **0/21** | **0/27** | **6/6** | 21/21 |
| Rewrite the whole file after changing the value | 0/21 | 21/21 | 3/27 | 3/6 | 19/21 |
| Replace the first line matching the field name | 6/21 | 6/21 | 10/27 | 2/6 | 8/21 |

- **Collateral damage** means "zero byte change outside the edit", which takes a different
  shape per operation: for `set` the prefix and suffix outside the value range must stay
  byte-identical; for `insert` **every byte of the original must survive**; for `delete`
  **only a span may be missing**. Comparing prefixes and suffixes alone is not enough —
  deleting an adjacent line together with the target is still one contiguous deletion — so
  `insert`/`delete` also compare the **set of key paths**: apart from the target, both
  documents must contain exactly the same fields.
- **Multi-edit plans** use a different rule: `expect.targets` declares each target, and the
  judge requires every target to hold, the key set to differ by exactly those targets, **every
  changed line to mention a target key**, and each `set` target's line to keep its prefix and
  suffix byte-identical (indentation, trailing comment and `#@` metadata included).
- **Edits across `@include`**: `expect.target_file` declares which file is expected to
  change; the strategy starts from the entry file and may follow includes, the judge compares
  against **that file's** content and requires the strategy to have changed exactly that file.
  The corpus hands the file tree to strategies in memory (no disk I/O), so strategies stay pure.
- **When a naive diff implementation is good enough**: `line-diff` gets single root-level
  inserts and deletes right (they need no scope information and touch no in-line detail), but it
  appends a key to the **end of the file** when inserting into a nested table (so the target does
  not exist at all), matches the **first** `weight` line when deleting a named-list element
  (removing the wrong element), its whole-line replacement eats the target line's trailing comment
  and `#@` metadata, and **3 of the 4 cross-`@include` tasks are refused outright**
  because it does not follow includes. These failure modes are recorded per task rather than
  summarised as "not good enough".
- **Refusal accuracy** requires the expected error code — refusing for the wrong reason does
  not count.
- The rewrite strategy uses this repository's own comment-preserving formatter. A real
  model rewrite would also drop comments, so the collateral damage it shows is a **lower
  bound**.
- All three strategies are byte-for-byte deterministic across repeated and reversed runs;
  the `--check` gate requires the implementation under test to be entirely correct and
  deterministic.

```bash
cargo run -p verseconf-bench --release            # run the benchmark and write results
cargo run -p verseconf-bench --release -- --check # gate mode (used by CI)
```

The output is `benchmark/results/latest.md` (human-readable) and
`benchmark/results/latest.json` (machine-readable, carries the fingerprint).
Methodology and scope limits: [benchmark/README.md](benchmark/README.md).

## 4. Integration

`verseconf-mcp` exposes five capabilities as tools a host can discover and call:

| Tool | Purpose |
|------|---------|
| `verseconf_validate` | Parse + schema validation, returning errors with line and column |
| `verseconf_audit` | Security audit with stable rule codes |
| `verseconf_apply_edit` | Byte-range minimal edit from an edit plan, validated twice before writing |
| `verseconf_edit_range` | Replace an explicit character range (a lower-level entry point) |
| `verseconf_check_write` | **Pre-write check**: given the original and candidate text, decide whether the change may be written. It does not care how the candidate was produced, so a host keeps its own editing method and only adds this gate before writing |

Failures return **structured refusal reasons**, not a paragraph of prose (see
[2.2](#22-the-refusal-paths) for the codes).

### Native binary

```bash
cargo install verseconf-mcp
verseconf-mcp --list-tools      # inspect the tool contract offline
```

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

### WebAssembly (hosts without a Rust toolchain)

The same tool implementations are compiled to WebAssembly and can be loaded by a JS
runtime, so the host needs no Rust toolchain. The build output also ships a stdio server,
`verseconf-mcp-wasm.mjs`.

**This distribution path is not published**: the npm package has been abandoned (see the
status table in [5. Installation](#5-installation)), so using it means building from source:

```bash
cd integrations/verseconf-wasm/js-api && npm install && npm run build
```

Both paths share the same functions and return byte-identical
responses for the same request (checked by `npm run test:parity`). **Sharing code is not the
same as having the same capabilities**: a wasm target has no filesystem, so `tools/list`
does not advertise `path` there and passing it returns `unsupported_on_platform`; to edit by
path, the host must read the file itself and pass the text as `source`.

The full tool contract is in [docs/MCP.md](docs/MCP.md).

## 5. Installation

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
| crates.io (four crates) | ✅ **v0.2.0 published.** `cargo install verseconf-cli --version 0.2.0` verified in a clean directory: the `edit` command works and a second instance of an already-present high-risk rule is refused. Do not use 0.1.0 — it predates every fix (its CLI has no `edit`, and its core compares safety findings by rule rather than by instance) |
| API docs (docs.rs) | ✅ [docs.rs/verseconf-core](https://docs.rs/verseconf-core) |
| VSCode extension `.vsix` | ⚠️ Produced and uploaded by CI as a build artifact; the repository contains no binary. See [9. Editor support](#9-editor-support) |
| npm package `verseconf` | ❌ **Publishing abandoned**. The 0.1.0 on the registry does not work (the tarball has no `pkg/` directory, so both `require` and `import` fail); the fixed 0.2.0 is built in-repo and passes packaging checks (12/12), but the publishing account is no longer available, so it will not be published |
| Browser CDN | ❌ Not published. The `pkg-web/` output is in no published package |

## 6. Quick start

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

## 7. CLI reference

| Command | Purpose |
|---------|---------|
| `verseconf parse <file>` | Parse and print the structure; `--no-include` disables `@include` expansion |
| `verseconf validate <file>` | Validate including schema; `--strict` rejects undeclared fields; `--fix --write` applies safe fixes |
| `verseconf format <file>` | Format; `-o` writes to a file; `--ai-canonical` for canonical form; `--include` merges includes; `--env` interpolates environment variables |
| `verseconf audit <file>` | Security audit (wildcard binds, plaintext credentials, …) with stable rule codes |
| `verseconf doc <file>` | Generate documentation from the schema |
| `verseconf schema generate <file>` | **Infer** a `#@schema` block from a config; prints by default, `--write` writes it back (refused when a schema already exists) |
| `verseconf diff <a> <b>` | Compare two configurations |
| `verseconf env <subcommand>` | Environment management |
| `verseconf template <subcommand>` | `render` / `list` / `validate` / `generate` |
| `verseconf version <subcommand>` | Version management |
| `verseconf edit <file> --plan <plan.json>` | Apply an edit-intent plan, **locating the target across `@include` files**; prints by default, `--write` persists (re-reads and compares content before replacing atomically; refuses to write when the merged view is unvalidated unless `--allow-unvalidated`) |
| `verseconf watch <file>` | Watch a file and re-validate it on every change (including a security-audit summary); `--max-events N` exits after N changes |

Three properties worth calling out:

- `format` is **idempotent** and preserves comments and `#@` metadata;
- `validate --fix` makes **zero byte-level changes** to an already-valid file;
- `watch` complements the editor's live diagnostics: the LSP covers "editing in an editor",
  while `watch` covers the **no-editor** cases — an agent rewriting files in the background,
  a script generating config, or CI waiting for one save. Both success and failure are printed:
  a parse failure reports `line:column` and the reason instead of keeping the last good result:

  ```
  [watch] watching app.vcf (re-validates on change; Ctrl-C to quit)
  [watch] 10:24:01  OK: 2 root entries, no audit findings
  [watch] 10:24:07  2:5: parse error: expected '=' or ':', found bare_key(line)
  [watch] 10:24:11  OK: 2 root entries, no audit findings
  ```

## 8. Configuration language reference

> This section is a reference, not the main line. The language only carries the intent:
> what this project commits to is [2. The intent execution protocol](#2-the-intent-execution-protocol)
> and [3. Measured results](#3-measured-results-edit-fidelity), not expressive power.

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
| `${VAR\|default}` | Environment interpolation. **Must be inside a quoted string**, and needs `--env`; the recommended default separator is `\|`, with `:` and ` or ` also accepted (see below) |
| `{{VAR}}` | Template variables, used with `verseconf template render/list/validate/generate` |

Errors carry `file:line:column` plus context, for example:

```
app.vcf:2:3: validation error: value 99999 for field 'port' exceeds maximum 65535
```

### Not implemented

> This section exists because earlier documentation followed the spec and described
> syntax that **the current implementation does not support**. The forms below appear in
> the spec, but copying them will fail or produce a value that is not what the spec meant.

| Form | Actual behaviour |
|------|------------------|
| Bare expressions such as `${port + 1}` or `${30s + 500ms}` | **Lexical error** `unexpected character: '$'` (`parse_error`). Inside a quoted string it does not error, but that goes through environment interpolation (needs `--env`) and is **not expression evaluation**; the AST has `BinaryOp` / `UnitValue` and an evaluator, but no syntax can produce them |
| Bare `${ENV:HOSTNAME}` | The **bare form** is a lexical error, same as above. **Inside quotes it does parse**, so it is no longer a lexical error; but `:` is the interpolator's legacy "variable:default" separator, so `"${ENV:HOSTNAME}"` actually resolves as "variable `ENV`, default `HOSTNAME`" — **not** "read the environment variable `HOSTNAME`". Copying it does not error, but the value you get is not what the spec meant |
| `#@template` / `#@use` template inheritance | Not implemented. Templates are `{{VAR}}` plus the `verseconf template` subcommands |

The conclusions above were measured on this machine: `validate` returns `valid: true`
for quoted `"${ENV:HOSTNAME}"`, `"${VAR:default}"` and
`"${VAR or default}"`, and `parse_error` + `unexpected character: '$'` for the bare
forms. All three default separators parse (implementation:
`crates/verseconf-core/src/semantic/env_interp.rs`), but the documentation recommends
`${VAR|default}` only.

The full grammar reference is [docs/SPECIFICATION.md](docs/SPECIFICATION.md); for anything
marked unimplemented there, this section is the authority.

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

All three commands are backed by **real capabilities**, with no placeholder paths left:

| Command | What it actually does |
|---------|----------------------|
| `verseconf.format` | Served by the language server's `documentFormattingProvider`, reusing the core library's comment-preserving formatter: comments and `#@` metadata survive, and the result is idempotent. When the document cannot be parsed it makes **no change at all** rather than guessing |
| `verseconf.validate` | Reads the live diagnostics published by the language server (the same ones you see in the editor), reports the first problem's line and column, and can jump to the Problems panel. If the server is not running it says so instead of falsely reporting "valid" |
| `verseconf.schema.generate` | Infers a `#@schema { ... }` block from the current document and inserts it at the top. Inference writes only types and `required` that follow from the values, and **never invents** `range` / `enum` / `default` / `pattern`. Refuses when the document already has a `#@schema` (two schemas in one file fail to parse) |

Inference lives in the core library (`infer_schema`) and is exposed to the extension as
the custom request `verseconf/generateSchema` — the extension needs nothing beyond the
language server it already ships with.

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
cargo test -p verseconf-test --test spec_status               # spec status gate
```

CI has 9 **blocking** jobs: `build-and-test`, `format`, `clippy`, `examples`,
`distribution`, `spec-status`, `fidelity-benchmark`, `performance-benchmark` and
`wasm-distribution`, plus 5 jobs in the extension pipeline.

`spec-status` keeps the status markers in
[docs/SPECIFICATION.md](docs/SPECIFICATION.md) from drifting again: it runs every
claim in [docs/spec-status-probes.json](docs/spec-status-probes.json) through a real
`parse` / `validate` call and cross-checks whether the quick-reference table lists
that construct under ✅ implemented or 🚧 not implemented. Both "someone implemented a
🚧 feature" and "someone broke a ✅ feature" fail this job.

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
| [docs/SPECIFICATION.md](docs/SPECIFICATION.md) | Language specification (with implemented/unimplemented status markers) |
| [docs/TUTORIAL.md](docs/TUTORIAL.md) | Getting-started tutorial |
| [docs/MCP.md](docs/MCP.md) | Tool-protocol contract, error-code table and host integration |
| [crates/verseconf-core/schemas/edit-plan.schema.json](crates/verseconf-core/schemas/edit-plan.schema.json) | **The edit intent contract (public schema)** |
| [benchmark/README.md](benchmark/README.md) | Methodology and scope limits of the edit fidelity benchmark |
| [crates/verseconf-core/README.md](crates/verseconf-core/README.md) | Core API and the minimal-edit example |
| [crates/verseconf-cli/README.md](crates/verseconf-cli/README.md) | Command-line usage |
| [crates/verseconf-mcp/README.md](crates/verseconf-mcp/README.md) | Tool-protocol server |
| [crates/verseconf-lsp/README.md](crates/verseconf-lsp/README.md) | Language server |
| [extensions/verseconf-vscode/README.md](extensions/verseconf-vscode/README.md) | Extension development and packaging |
| [compare/performance_charts.md](compare/performance_charts.md) | Parse performance report (generated from the benchmark, see Appendix A) |
| [examples/](examples/) | Runnable example configurations |

## 13. Known limitations and roadmap

**Current limitations**

1. **Edits across `@include` are implemented in the core library, `verseconf edit`,
   the tool protocol and the benchmark** (fail-closed location: a target that resolves in more
   than one file is refused). The corpus currently uses a single include level; deeper chains and
   "targets inside a table produced by merging several includes" are not covered yet.
2. The benchmark uses deterministic stand-in strategies and no real model — it measures
   the inherent cost of an approach, not a particular model's score.
3. **JS / WebAssembly distribution is not published**: the npm package has been abandoned
   (the publishing account is unavailable) and no browser CDN build exists. Both paths are
   build-from-source only; the CLI and the libraries ship via crates.io.
4. **Parse performance is not this project's advantage**: it is 2.77–5.65x slower than
   `serde_json` (see Appendix A). The evidence for this project is edit fidelity, not
   parse speed.
5. **No net benefit was measured, and the project has been closed out as a "small reliable
   configuration-editing library"** per its preregistration: the confirmatory replication
   (1,422 real model runs on frozen holdout documents, against thresholds fixed before the
   run) **failed the hard gates** — 88.8% semantic / 88.8% byte fidelity, below both controls
   (97.5% / 90.7% and 96.8% / 90.7%); 10 silent mis-edits (controls: 9 / 0); model-side cost
   8.9% higher. The only positive evidence is on the safety side: the ablation shows the
   validation gate blocked all 5 edits that would have introduced high-risk instances (the
   control arm wrote every one of them silently), with 0 false refusals across 79 neutral
   tasks. See [benchmark/confirmation/VERDICT.md](benchmark/confirmation/VERDICT.md) and
   [benchmark/ablation/](benchmark/ablation/).

**Roadmap**

- Extend the cross-`@include` corpus to deeper nesting and to tables produced by merging
  several includes (core library, CLI, tool protocol and benchmark already cover one level, see section 3)
- **No longer planned**: publishing the npm package. The fixed 0.2.0 build stays in the repo
  and its build and packaging checks run in CI, but the publishing account is unavailable
- Broaden schema inference (for example, inferring a shared schema across several files)

## Appendix A: parse performance

> This section is unrelated to the "agents editing configuration" main line. It is kept
> only to answer "what does parsing itself cost". **It is not this project's evidence** —
> that is [3. Measured results](#3-measured-results-edit-fidelity). On the "who parses
> fastest" axis VerseConf does not win, and we say so.

One release run on this machine (Windows 10 / AMD64 / rustc 1.98.1, median of 5 rounds per
format):

| Dataset | VerseConf | TOML | JSON | TOML/VCF | VCF/JSON |
|---------|-----------|------|------|----------|----------|
| small (426B) | 9.55μs | 13.51μs | 1.96μs | 1.42x | 4.88x |
| medium (2.5KB) | 78.51μs | 96.99μs | 13.89μs | 1.24x | 5.65x |
| large (24.8KB) | 756.34μs | 950.42μs | 197.13μs | 1.26x | 3.84x |
| xlarge (263KB) | 6.82ms | 8.49ms | 2.46ms | 1.25x | 2.77x |

- VerseConf parses roughly **1.24–1.42x faster than TOML**;
- `serde_json` is faster than all three, and VerseConf is **2.77–5.65x slower** than it
  (the largest gap is on the 2.5KB dataset, the smallest on the 263KB one);
- all three formats carry the same configuration written equivalently.

```bash
cargo run -p verseconf-compare --bin generate_test_data
cargo run --release -p verseconf-compare --bin benchmark -- --json compare/benchmark_results.json
python3 compare/generate_charts.py
```

The single source for these numbers is `compare/benchmark_results.json`. The chart script
contains no fallback data, and the benchmark exits non-zero rather than reporting zeros
when data is missing or fails to parse. Full report:
[compare/performance_charts.md](compare/performance_charts.md).

## License

MIT OR Apache-2.0 — see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
