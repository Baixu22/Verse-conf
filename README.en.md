# VerseConf

[简体中文](README.md) | **English**

> **Frozen · 2026-09-28**
>
> VerseConf is no longer developed as an independent product line. This repository preserves
> configuration-editing and pre-write-check implementations and their experimental record.
> No further `.vcf`, intent-protocol, LSP, VSCode, WebAssembly or distribution expansion is planned.
> No future release or maintenance response time is promised. Net benefit in a real host remains
> unproven; the existing gate is not recommended as a default security boundary.

[![Status](https://img.shields.io/badge/status-frozen-lightgrey)](docs/CLOSEOUT.md)
[![License](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-green.svg)](LICENSE)

## Closeout decision

- **Stop product expansion and new model experiments.** There is no planned sixth round or new format adapter.
- **Keep source, tests, raw results and negative conclusions.** Freezing does not mean deleting evidence or rewriting failures as successes.
- **Stop automatic CI, packaging and distribution artifact production.** Only manual evidence verification and Rust regression checks remain; no VSIX / wasm distribution matrix runs.
- **Do not make host integration a prerequisite for closure.** No host has been modified. A future consumer may reuse the differential-checking idea without adopting this repository.
- **No remote archival, commit, push, publication or package withdrawal was performed.** “Frozen” describes this source tree's maintenance decision, not the GitHub archived setting.

See the [final closeout decision](docs/CLOSEOUT.md) for evidence and scope, and
[CONTRIBUTING.md](CONTRIBUTING.md) for reopening constraints.

## Implemented does not mean worth adopting

The source contains:

- VCF path resolution, `expect` preconditions, local source edits and structured refusal codes.
- TOML and JSON / JSONC adapters using `toml_edit` / `jsonc-parser`.
- `check_write(baseline, candidate)`, independent of the model's editing protocol, and optional standard JSON Schema validation for JSON.
- CLI, MCP, LSP, VSCode and wasm wrappers. These demonstrate callable interfaces, not adoption by a business application.

“Minimal edit” means preserving source outside the supported edit's target region. It does not mean
minimal disk writes, historical rollback or multi-file transactions. `check_write` judges a candidate
already held by its caller; the host must enforce the result. An optional MCP call is not an unavoidable write barrier.

## Experimental conclusions

The confirmatory experiment measured **TOML scalar-set editing interfaces**, not the `.vcf` language,
an end-to-end agent loop or production deployment. Its 1,422 runs came from 79 tasks and 16 sources,
with 474 runs per arm; they were not 1,422 independent tasks.

| Editing interface | Semantic correctness | Strict byte fidelity |
| --- | ---: | ---: |
| String replacement | 462/474 (97.5%) | 459/474 (96.8%) |
| Mature span editor | 430/474 (90.7%) | 430/474 (90.7%) |
| VerseConf intent | 421/474 (88.8%) | 421/474 (88.8%) |

Intent used approximately 8.9% more model tokens per strictly correct result than span editing,
failing the original net-benefit target. This supports ending promotion of that protocol, not a claim
that string replacement is universally superior. See the [verdict](benchmark/confirmation/VERDICT.md)
and [source-cluster report](benchmark/confirmation/results/cluster-report.md).

The five later rounds tested a safety gate on existing TOML, reusing the same 8 documents and
12 tasks rather than providing five independent validations. Round 5 had 72 generation opportunities,
60 successful generations and 12 failures. Under the frozen outcome predicate, the risk count fell
from 9 to 4, blocking 5 candidates. This is local interception evidence; **task completion after refusal,
less manual repair and lower total cost were not demonstrated**.

The historical round-5 “increment” verdict is not a product acceptance pass:

- Preregistration required a significant reduction; the runner only compared `B < A` without the corresponding significance test.
- The original freeze did not bind the runner, classifier or tested binary. The current shared runner cannot guarantee reproduction of every historical aggregation rule.
- “0/68 implementation false positives” is not 68 independent model reviews. Even the zero-event approximation gives an upper bound near 4.4%, insufficient to establish a ≤2% rate.
- Policy refusals still impose user costs. Confirming a text pattern is not confirming its application semantics.

The [historical round-5 verdict](benchmark/gate-increment-round5/VERDICT.md) remains unchanged.
The [final decision](docs/CLOSEOUT.md) adds qualifications without revising raw measurements.

## Safety boundaries

| Case | Current behavior and limitation |
| --- | --- |
| Introduce `tls_verify=false` | SEC-005 refuses it |
| Change `verify_email=true` to `false` | Also refused by SEC-005: matching uses `ssl` / `verify` substrings in paths, not knowledge of TLS fields |
| Introduce `NODE_TLS_REJECT_UNAUTHORIZED="0"` | Not covered by the current rule; may be allowed |
| Change an integer to a string without a schema | May be allowed; type preservation is not guaranteed |
| Replace configuration with `{}` without a schema | May be allowed; task completion and application validity are not guaranteed |
| Supply a schema | Only the supplied or explicitly enabled schema is checked, not the real application's loader |
| Replace an existing risk at the same rule and location | The instance comparison may not count it as newly introduced; this is not a complete non-worsening guarantee |
| Use text such as `${ENV_VAR}` | Acceptance does not establish that the target application supports interpolation or that the safe alternative works |
| YAML | Not supported by the current gate |

[Known-boundary tests](crates/verseconf-json/tests/known_boundaries.rs) make selected limitations
executable. They characterize the frozen version; they do not endorse false refusals or missed risks as correct policy.

The CLI also has a concurrency window between its pre-write reread and rename. It does not provide
strict compare-and-swap, multi-file transactions, persistent rollback history or complete security.

## Historical distribution status

No registry was rechecked online and no published version was changed during closeout.

| Channel | Closeout status |
| --- | --- |
| crates.io | Historical records report core / cli / lsp / mcp / toml 0.3.0 published; no further release is planned. Version 0.1.0 predates important fixes and is not recommended |
| JSON / JSONC | Source-tree implementation, unpublished; registry MCP 0.3.0 does not contain it. The “next release” plan is cancelled |
| npm | Publishing abandoned; the historical 0.1.0 package lacks wasm output and is not a working entry point |
| VSCode / wasm / CDN | Continuous distribution builds stopped. Historical source and local build instructions remain; old Actions artifacts are not guaranteed to remain downloadable |
| Docker | The Dockerfile runs `fortune`; it is a placeholder, not a VerseConf image. No replacement image will be built or distributed |

## Verification and reproduction

From the repository root:

```bash
node benchmark/verify-closeout.mjs
node --test benchmark/analysis/cluster_report.test.mjs benchmark/gate-increment/classify.test.mjs
cargo test --workspace --offline --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
```

Offline Cargo commands require cached dependencies and an installed toolchain (including MSVC build
tools on Windows). Manual GitHub checks may download build dependencies, but do not call model APIs,
publish packages or upload distribution artifacts. The SHA-256 manifest is a **closeout-time snapshot
on 2026-09-28**, not a retroactive preregistration or proof of the binaries used in historical runs.

## Reference material

- [Final closeout decision](docs/CLOSEOUT.md) · [Contribution and reopening constraints](CONTRIBUTING.md)
- [Original README and workflow snapshots](docs/archive/2026-09-28/README.md) (byte-preserved `.txt` files, historical only)
- [Language specification](docs/SPECIFICATION.md) · [Historical tutorial](docs/TUTORIAL.md) · [MCP contract](docs/MCP.md)
- [Edit-fidelity benchmark](benchmark/README.md) · [Confirmatory experiment](benchmark/confirmation/VERDICT.md)
- [Core](crates/verseconf-core/README.md) · [TOML](crates/verseconf-toml/README.md) · [JSON](crates/verseconf-json/README.md)
- [VSCode source](extensions/verseconf-vscode/README.md) · [wasm source](integrations/verseconf-wasm/README.md)

The license remains [MIT OR Apache-2.0](LICENSE).
