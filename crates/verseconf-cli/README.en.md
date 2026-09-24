# verseconf-cli

[简体中文](README.md) | **English**

The command-line tool for the VerseConf configuration language.

> **The package name and the command name differ**: the package is `verseconf-cli`,
> and the installed executable is `verseconf`. This is the same arrangement as the
> `ripgrep` package installing the `rg` command.

## Install

```bash
cargo install verseconf-cli
```

## Commands

| Command | Purpose |
|---|---|
| `verseconf parse <file>` | Parse and print the structure (`--no-include` disables `@include` expansion) |
| `verseconf validate <file>` | Validate including schema; `--fix --write` makes zero byte-level changes to a valid file |
| `verseconf format <file>` | Format, preserving comments and `#@` metadata; idempotent |
| `verseconf audit <file>` | Security audit (wildcard binds, plaintext credentials, …) |
| `verseconf doc <file>` | Generate documentation from the schema |
| `verseconf diff <a> <b>` | Compare two configurations |
| `verseconf env <subcommand>` | Environment management |
| `verseconf template <subcommand>` | Template commands |
| `verseconf version <subcommand>` | Version management |

## Examples

```bash
# Parse
verseconf parse config.vcf

# Validate; --fix changes nothing in an already-valid file
verseconf validate config.vcf
verseconf validate config.vcf --fix --write

# Format (two runs produce identical output)
verseconf format config.vcf -o formatted.vcf

# Security audit
verseconf audit config.vcf

# Environment interpolation: ${DB_HOST|localhost}
verseconf format config.vcf --env
```

## See also

- [Main repository and language documentation](https://github.com/Baixu22/Verse-conf)
- [Core library verseconf-core](https://crates.io/crates/verseconf-core)

## License

MIT OR Apache-2.0
