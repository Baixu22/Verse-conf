# VerseConf VSCode Extension

VSCode extension for VerseConf language support.

## Features

- **Syntax Highlighting**: Full syntax highlighting for .vcf files
- **Auto-completion**: Context-aware completion for keys and values
- **Hover Information**: Hover docs for keys and types
- **Validation**: Real-time error detection and diagnostics
- **Formatting**: Document formatting support
- **LSP Integration**: Full Language Server Protocol support

## Requirements

- VSCode 1.75.0 or later
- VerseConf LSP server (built from `crates/verseconf-lsp`)

## Building the LSP Server

Before using the extension, you need to build the LSP server:

```bash
cd crates/verseconf-lsp
cargo build --release
```

The LSP binary will be at `crates/verseconf-lsp/target/release/verseconf-lsp.exe`

## Installation

1. Clone the repository
2. Install Node.js dependencies:
   ```bash
   cd extensions/verseconf-vscode
   npm install
   ```
3. Compile TypeScript:
   ```bash
   npm run compile
   ```
4. Open the project in VSCode and press `F5` to debug

## Extension Structure

```
extensions/verseconf-vscode/
├── package.json              # Extension manifest
├── language-configuration.json  # Language settings
├── syntaxes/
│   └── verseconf.tmLanguage.json  # TextMate grammar
├── src/
│   └── extension.ts          # Extension entry point
└── tsconfig.json             # TypeScript config
```

## Configuration

The extension provides the following settings:

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `verseconf.format.aiCanonical` | boolean | false | AI-friendly formatting |
| `verseconf.lsp.enabled` | boolean | true | Enable LSP features |
| `verseconf.validation.strict` | boolean | false | Strict validation mode |

## Commands

| Command | Description |
|---------|-------------|
| `verseconf.format` | Format the current document |
| `verseconf.validate` | Validate the current document |
| `verseconf.schema.generate` | Generate schema from document |

## Keyboard Shortcuts

The extension integrates with VSCode's standard formatting command:

- `Shift+Alt+F` (Windows/Linux) or `Shift+Option+F` (macOS) - Format document

## Development

### Debugging

1. Open `extensions/verseconf-vscode` in VSCode
2. Run `npm install`
3. Press `F5` to start debugging
4. The extension will be loaded in a new Extension Development Host window

### Building for Production

```bash
npm run vscode:prepublish
```

This compiles TypeScript to JavaScript in the `out` folder.

## License

MIT OR Apache-2.0
