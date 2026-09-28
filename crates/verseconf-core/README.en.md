# verseconf-core

> **Frozen (2026-09-28)**: Independent product development has stopped; no further releases or ongoing maintenance are promised. The content below is historical source reference only. See [CLOSEOUT.md](../../docs/CLOSEOUT.md) for security boundaries and the final decision.

[简体中文](README.md) | **English**

The core library for the VerseConf configuration language: parsing, AST, validation,
security audit, and a **byte-range minimal edit** engine.

Its historical aim was a **deterministic execution layer for agents editing configuration**:
models supplied semantic intent (which field, what value, why), and deterministic code
resolved and replaced byte ranges. Schema validation is optional and security auditing
uses heuristic rules; passing these checks does not establish safety or guarantee refusal
of every uncertain case. Existing evidence has not demonstrated a net benefit in real use.

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

When a precondition fails, the target is ambiguous, an enabled schema check fails, or
heuristic auditing flags a newly introduced high-risk instance, `apply_edit_plan` returns
`EditRefusal`; callers must not write a refused result. Problems outside those rules can
still pass.

A complete runnable version:
[`crates/verseconf-core/examples/deterministic_edit.rs`](https://github.com/Baixu22/Verse-conf/blob/main/crates/verseconf-core/examples/deterministic_edit.rs).
It is also part of `cargo test --workspace`, so the example in this README cannot drift
from the implementation.

## Public benchmark: does editing a config change anything else?

The repository ships an edit fidelity benchmark a third party can re-run (8 documents,
27 tasks covering `set` / `insert` / `delete`, multi-edit plans and edits across
`@include`, no network and no model):

| Strategy | Correct | Collateral damage | Refusal accuracy |
|---|---|---|---|
| Intent contract + byte-range minimal edit | 21/21 | **0/21** | 6/6 |
| Rewrite the whole file after changing the value | 0/21 | 21/21 | 3/6 |
| Replace the first line matching the field name | 6/21 | 6/21 | 2/6 |

The corpus, the judge and all three strategies live in the repository; re-running produces
the same corpus fingerprint `162777399290a695`. Methodology and scope limits:
[`benchmark/README.md`](https://github.com/Baixu22/Verse-conf/blob/main/benchmark/README.md).

## See also

- [Main repository and command line](https://github.com/Baixu22/Verse-conf)
- [Tool-protocol server verseconf-mcp](https://crates.io/crates/verseconf-mcp) — exposes validation, audit and editing to agent hosts
- [Language server verseconf-lsp](https://crates.io/crates/verseconf-lsp)

## License

MIT OR Apache-2.0
