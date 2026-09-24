# verseconf-core

[简体中文](README.md) | **English**

The core library for the VerseConf configuration language: parsing, AST, validation,
security audit, and a **byte-range minimal edit** engine.

This project is not another attempt at inventing a configuration syntax. It is a
**deterministic execution layer for agents editing configuration**: the model supplies
only semantic intent (which field, what value, why), deterministic code resolves and
replaces the byte range, schema validation and a security audit run before anything is
written, and **when it cannot be certain it refuses instead of guessing**.

## Install

```bash
cargo add verseconf-core
```

## Quick start

```rust
use verseconf_core::{format, parse, validate_ast};

let source = "server {\n  port = 8080 # production port\n  host = \"127.0.0.1\"\n}\n";

let ast = parse(source).expect("should parse");
validate_ast(&ast).expect("should validate");

// Formatting preserves comments and #@ metadata, and is idempotent
println!("{}", format(source).expect("should format"));
```

## Change only the target value; every other byte stays identical

This is the line between this library and "let the model rewrite the whole file". The
caller supplies intent, not a new file:

```rust
use verseconf_core::{apply_edit_plan, EditPlan};

let source = "server {\n  port = 8080 # production port\n  host = \"127.0.0.1\"\n}\n";

let plan: EditPlan = serde_json::from_str(r#"{
  "version": "1.0",
  "edits": [
    { "op": "set", "path": ["server", "port"], "value": 9090,
      "expect": { "value": 8080 } }
  ]
}"#).expect("the plan should be valid");

let outcome = apply_edit_plan(source, &plan).expect("should be accepted");

assert!(outcome.source.contains("9090"));            // the target value changed
assert!(outcome.source.contains("# production port")); // the trailing comment survived
assert!(outcome.source.contains("127.0.0.1"));       // bytes outside the range are untouched
```

When a precondition does not hold, the target is ambiguous, or the edit would break the
schema or introduce a new security risk, `apply_edit_plan` returns `EditRefusal` rather
than guessing — and the caller must not write any file.

A complete runnable version:
[`crates/verseconf-core/examples/deterministic_edit.rs`](https://github.com/Baixu22/Verse-conf/blob/main/crates/verseconf-core/examples/deterministic_edit.rs).
It is also part of `cargo test --workspace`, so the example in this README cannot drift
from the implementation.

## Public benchmark: does editing a config change anything else?

The repository ships an edit fidelity benchmark a third party can re-run (6 documents,
14 tasks, no network and no model):

| Strategy | Correct | Collateral damage | Refusal accuracy |
|---|---|---|---|
| Intent contract + byte-range minimal edit | 8/8 | **0/8** | 6/6 |
| Rewrite the whole file after changing the value | 0/8 | 8/8 | 3/6 |
| Replace the first line matching the field name | 2/8 | 3/8 | 2/6 |

The corpus, the judge and all three strategies live in the repository; re-running produces
the same corpus fingerprint `115811767f1177d0`. Methodology and scope limits:
[`benchmark/README.md`](https://github.com/Baixu22/Verse-conf/blob/main/benchmark/README.md).

## See also

- [Main repository and command line](https://github.com/Baixu22/Verse-conf)
- [Tool-protocol server verseconf-mcp](https://crates.io/crates/verseconf-mcp) — exposes validation, audit and editing to agent hosts
- [Language server verseconf-lsp](https://crates.io/crates/verseconf-lsp)

## License

MIT OR Apache-2.0
