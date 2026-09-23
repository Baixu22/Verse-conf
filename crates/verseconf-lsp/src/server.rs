use std::collections::HashMap;
use std::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use verseconf_core::{parse, VerseconfError};

pub struct VerseConfBackend {
    client: Client,
    documents: Mutex<HashMap<Url, String>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for VerseConfBackend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "verseconf-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".to_string(), "=".to_string()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                        SemanticTokensRegistrationOptions {
                            text_document_registration_options: TextDocumentRegistrationOptions {
                                document_selector: None,
                            },
                            semantic_tokens_options: SemanticTokensOptions {
                                work_done_progress_options: WorkDoneProgressOptions::default(),
                                legend: SemanticTokensLegend {
                                    token_types: vec![
                                        SemanticTokenType::KEYWORD,
                                        SemanticTokenType::STRING,
                                        SemanticTokenType::NUMBER,
                                        SemanticTokenType::COMMENT,
                                        SemanticTokenType::PROPERTY,
                                        SemanticTokenType::TYPE,
                                    ],
                                    token_modifiers: vec![],
                                },
                                range: Some(true),
                                full: Some(SemanticTokensFullOptions::Bool(true)),
                            },
                            static_registration_options: StaticRegistrationOptions::default(),
                        },
                    ),
                ),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "verseconf-lsp initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file opened")
            .await;
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();
        self.documents
            .lock()
            .unwrap()
            .insert(uri.clone(), text.clone());
        self.validate_text(&uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        // 服务端声明的是 INCREMENTAL 同步，因此必须真正按 range 应用增量，
        // 否则每次按键都会把整篇文档替换成那一小段文本。
        let updated = {
            let mut documents = self.documents.lock().unwrap();
            let mut text = documents.get(&uri).cloned().unwrap_or_default();

            for change in &params.content_changes {
                match &change.range {
                    // 没有 range 表示整篇替换
                    None => text = change.text.clone(),
                    Some(range) => {
                        let start = position_to_offset(&text, range.start);
                        let end = position_to_offset(&text, range.end);
                        if start <= end && end <= text.len() {
                            text.replace_range(start..end, &change.text);
                        }
                    }
                }
            }

            documents.insert(uri.clone(), text.clone());
            text
        };

        self.validate_text(&uri, &updated).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        self.client
            .log_message(
                MessageType::LOG,
                format!("completion at {:?}:{:?}", uri, position),
            )
            .await;

        let mut items = vec![
            CompletionItem {
                label: "server".into(),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some("Server configuration block".into()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: "Configure server settings\n\n```verseconf\nserver {\n    host = \"localhost\"\n    port = 8080\n}\n```".into(),
                })),
                ..Default::default()
            },
            CompletionItem {
                label: "database".into(),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some("Database configuration block".into()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: "Configure database settings\n\n```verseconf\ndatabase {\n    host = \"localhost\"\n    port = 5432\n}\n```".into(),
                })),
                ..Default::default()
            },
            CompletionItem {
                label: "port".into(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some("Port number (integer)".into()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: "**port** `integer`\n\nThe port number for the service.".into(),
                })),
                ..Default::default()
            },
            CompletionItem {
                label: "host".into(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some("Host address (string)".into()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: "**host** `string`\n\nThe host address for the service.".into(),
                })),
                ..Default::default()
            },
            CompletionItem {
                label: "timeout".into(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some("Timeout in seconds (integer)".into()),
                ..Default::default()
            },
            CompletionItem {
                label: "debug".into(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some("Enable debug mode (boolean)".into()),
                ..Default::default()
            },
        ];

        if let Some(doc) = self.documents.lock().unwrap().get(&uri) {
            let symbols = extract_symbols(doc);
            for symbol in symbols {
                let name = symbol.name.clone();
                let value = symbol.value.clone();
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some(format!("Defined in document: {}", value)),
                    documentation: Some(Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("**{}** = `{}`\n\nDefined in current document", name, value),
                    })),
                    ..Default::default()
                });
            }
        }

        Ok(Some(CompletionResponse::List(CompletionList {
            is_incomplete: false,
            items,
        })))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(
                MessageType::LOG,
                format!("hover at {:?}:{:?}", uri, position),
            )
            .await;

        if let Some(doc) = self.documents.lock().unwrap().get(&uri) {
            if let Some(symbol) = find_symbol_at_position(doc, position) {
                let value_preview = if symbol.value.len() > 100 {
                    format!("{}...", &symbol.value[..100])
                } else {
                    symbol.value.clone()
                };

                let type_info = infer_type(&symbol.value);

                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!(
                            "## **{}**\n\n**Type:** `{}`\n\n**Value:** ```\n{}\n```\n\n---\n*Defined in document*",
                            symbol.name,
                            type_info,
                            value_preview
                        ),
                    }),
                    range: Some(symbol.range),
                }));
            }

            let line = doc.lines().nth(position.line as usize).unwrap_or("");
            if line.trim().starts_with('#') {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("## Comment\n\n```\n{}\n```", line.trim()),
                    }),
                    range: None,
                }));
            }
        }

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "## VerseConf\n\n**A modern configuration language for the AI era.**\n\n---\n\n### Quick Reference\n\n| Syntax | Description |\n|--------|-------------|\n| `key = value` | Simple assignment |\n| `# comment` | Comment line |\n| `block { }` | Nested block |".into(),
            }),
            range: None,
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(
                MessageType::LOG,
                format!("goto definition at {:?}:{:?}", uri, position),
            )
            .await;

        if let Some(doc) = self.documents.lock().unwrap().get(&uri) {
            if let Some(symbol) = find_symbol_at_position(doc, position) {
                if let Some(def_range) = find_symbol_definition(doc, &symbol.name) {
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location::new(
                        uri, def_range,
                    ))));
                }
            }
        }

        Ok(None)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        self.client
            .log_message(
                MessageType::LOG,
                format!("find references at {:?}:{:?}", uri, position),
            )
            .await;

        if let Some(doc) = self.documents.lock().unwrap().get(&uri) {
            if let Some(symbol) = find_symbol_at_position(doc, position) {
                let locations = find_all_references(doc, &uri, &symbol.name);
                return Ok(Some(locations));
            }
        }

        Ok(None)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        self.client
            .log_message(MessageType::LOG, format!("document symbols for {:?}", uri))
            .await;

        if let Some(doc) = self.documents.lock().unwrap().get(&uri) {
            let symbols = extract_symbols(doc);
            let lsp_symbols: Vec<SymbolInformation> = symbols
                .into_iter()
                .map(|s| SymbolInformation {
                    name: format!("{} = {}", s.name, s.value),
                    kind: SymbolKind::FIELD,
                    tags: None,
                    #[allow(deprecated)]
                    deprecated: None,
                    location: Location::new(uri.clone(), s.range),
                    container_name: None,
                })
                .collect();

            return Ok(Some(DocumentSymbolResponse::Flat(lsp_symbols)));
        }

        Ok(None)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;

        self.client
            .log_message(MessageType::LOG, format!("semantic tokens for {:?}", uri))
            .await;

        if let Some(doc) = self.documents.lock().unwrap().get(&uri) {
            let tokens = tokenize_document(doc);
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: tokens,
            })));
        }

        Ok(None)
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let uri = params.text_document.uri;

        self.client
            .log_message(
                MessageType::LOG,
                format!("semantic tokens range for {:?}", uri),
            )
            .await;

        if let Some(doc) = self.documents.lock().unwrap().get(&uri) {
            let tokens = tokenize_document(doc);
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: tokens,
            })));
        }

        Ok(None)
    }
}

struct SymbolInfo {
    name: String,
    value: String,
    range: Range,
}

fn extract_symbols(text: &str) -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();
    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim();
            let value = trimmed[eq_pos + 1..].trim();
            if !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                let col_start = line.find(key).unwrap_or(0);
                symbols.push(SymbolInfo {
                    name: key.to_string(),
                    value: value.to_string(),
                    range: Range::new(
                        Position::new(line_idx as u32, col_start as u32),
                        Position::new(line_idx as u32, (col_start + key.len()) as u32),
                    ),
                });
            }
        }
    }
    symbols
}

fn find_symbol_at_position(text: &str, position: Position) -> Option<SymbolInfo> {
    let symbols = extract_symbols(text);
    symbols
        .into_iter()
        .find(|s| s.range.start.line <= position.line && s.range.end.line >= position.line)
}

fn find_symbol_definition(text: &str, name: &str) -> Option<Range> {
    extract_symbols(text)
        .into_iter()
        .find(|s| s.name == name)
        .map(|s| s.range)
}

fn find_all_references(text: &str, uri: &Url, name: &str) -> Vec<Location> {
    let mut locations = Vec::new();
    for (line_idx, line) in text.lines().enumerate() {
        let mut start = 0;
        while let Some(pos) = line[start..].find(name) {
            let abs_pos = start + pos;
            if abs_pos + name.len() <= line.len() {
                let before = if abs_pos > 0 { &line[..abs_pos] } else { "" };
                let after = &line[abs_pos + name.len()..];

                let is_word_boundary = (before.is_empty()
                    || !before.chars().last().unwrap().is_alphanumeric())
                    && (after.is_empty() || !after.chars().next().unwrap().is_alphanumeric());

                if is_word_boundary {
                    locations.push(Location::new(
                        uri.clone(),
                        Range::new(
                            Position::new(line_idx as u32, abs_pos as u32),
                            Position::new(line_idx as u32, (abs_pos + name.len()) as u32),
                        ),
                    ));
                }
            }
            start = abs_pos + 1;
        }
    }
    locations
}

fn tokenize_document(text: &str) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for (line_idx, line) in text.lines().enumerate() {
        let current_line = line_idx as u32;

        if line.trim().starts_with('#') {
            if let Some(start) = line.find('#') {
                let delta_line = current_line - prev_line;
                let delta_start = if delta_line == 0 {
                    start as u32 - prev_start
                } else {
                    start as u32
                };
                tokens.push(SemanticToken {
                    delta_line,
                    delta_start,
                    length: (line.len() - start) as u32,
                    token_type: 3,
                    token_modifiers_bitset: 0,
                });
                prev_line = current_line;
                prev_start = start as u32;
            }
            continue;
        }

        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim();
            if let Some(key_start) = line.find(key) {
                let delta_line = current_line - prev_line;
                let delta_start = if delta_line == 0 {
                    key_start as u32 - prev_start
                } else {
                    key_start as u32
                };
                tokens.push(SemanticToken {
                    delta_line,
                    delta_start,
                    length: key.len() as u32,
                    token_type: 4,
                    token_modifiers_bitset: 0,
                });
                prev_line = current_line;
                prev_start = key_start as u32;
            }

            let value = line[eq_pos + 1..].trim();
            if !value.is_empty() {
                if let Some(val_start) = line[eq_pos + 1..].find(value) {
                    let abs_val_start = eq_pos + 1 + val_start;
                    let delta_line = current_line - prev_line;
                    let delta_start = if delta_line == 0 {
                        abs_val_start as u32 - prev_start
                    } else {
                        abs_val_start as u32
                    };

                    let token_type = if value.starts_with('"') {
                        1
                    } else if value.chars().all(|c| c.is_numeric() || c == '.') {
                        2
                    } else {
                        0
                    };

                    tokens.push(SemanticToken {
                        delta_line,
                        delta_start,
                        length: value.len() as u32,
                        token_type,
                        token_modifiers_bitset: 0,
                    });
                    prev_line = current_line;
                    prev_start = abs_val_start as u32;
                }
            }
        }
    }

    tokens
}

fn infer_type(value: &str) -> &'static str {
    let trimmed = value.trim();
    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        "string"
    } else if trimmed == "true" || trimmed == "false" {
        "boolean"
    } else if trimmed.chars().all(|c| c.is_numeric() || c == '.') {
        if trimmed.contains('.') {
            "float"
        } else {
            "integer"
        }
    } else if trimmed.starts_with('[') || trimmed.starts_with('{') {
        "array/object"
    } else {
        "unknown"
    }
}

impl VerseConfBackend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Mutex::new(HashMap::new()),
        }
    }

    async fn validate_text(&self, uri: &Url, text: &str) {
        // 解析错误已经带有行列号，直接转成 Diagnostic 发出去。
        // 之前两个分支都发空数组，等于"实时校验"永远是空的。
        let diagnostics = match parse(text) {
            Ok(_) => Vec::new(),
            Err(error) => vec![diagnostic_from_error(&error, text)],
        };

        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }
}

/// 把 (line, character) 位置换算为字节偏移。
/// LSP 的 character 以 UTF-16 code unit 计。
fn position_to_offset(text: &str, position: Position) -> usize {
    let mut line_start = 0usize;

    for _ in 0..position.line {
        match text[line_start..].find('\n') {
            Some(index) => line_start += index + 1,
            None => return text.len(),
        }
    }

    let line_end = text[line_start..]
        .find('\n')
        .map(|index| line_start + index)
        .unwrap_or(text.len());
    let line = &text[line_start..line_end];

    let mut utf16_units = 0usize;
    for (byte_index, ch) in line.char_indices() {
        if utf16_units >= position.character as usize {
            return line_start + byte_index;
        }
        utf16_units += ch.len_utf16();
    }

    line_end
}

/// 字节偏移换算为 LSP 位置
fn offset_to_position(text: &str, offset: usize) -> Position {
    let offset = offset.min(text.len());
    let mut line = 0u32;
    let mut line_start = 0usize;

    for (index, ch) in text[..offset].char_indices() {
        if ch == '\n' {
            line += 1;
            line_start = index + 1;
        }
    }

    let character = text[line_start..offset]
        .chars()
        .map(|c| c.len_utf16() as u32)
        .sum();
    Position::new(line, character)
}

/// 把带位置的解析错误转成编辑器诊断
fn diagnostic_from_error(error: &VerseconfError, text: &str) -> Diagnostic {
    let span = error.span();

    let range = if span.is_unknown() {
        Range::new(Position::new(0, 0), Position::new(0, 0))
    } else {
        let start = Position::new(span.line.saturating_sub(1), span.column.saturating_sub(1));
        let end = if span.end > span.start {
            offset_to_position(text, span.end)
        } else {
            Position::new(start.line, start.character + 1)
        };
        Range::new(start, end)
    };

    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("verseconf".into()),
        message: error.message(),
        related_information: None,
        tags: None,
        data: None,
    }
}

pub async fn run() {
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(VerseConfBackend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_to_offset_ascii() {
        let text = "alpha\nbeta\ngamma\n";
        assert_eq!(position_to_offset(text, Position::new(0, 0)), 0);
        assert_eq!(position_to_offset(text, Position::new(1, 2)), 8);
        assert_eq!(position_to_offset(text, Position::new(2, 5)), 16);
    }

    #[test]
    fn test_position_to_offset_clamps_past_end_of_line() {
        let text = "alpha\nbeta\n";
        assert_eq!(position_to_offset(text, Position::new(0, 99)), 5);
        assert_eq!(position_to_offset(text, Position::new(9, 0)), text.len());
    }

    #[test]
    fn test_offset_to_position_roundtrip() {
        let text = "alpha\nbeta\ngamma\n";
        let position = offset_to_position(text, 8);
        assert_eq!(position, Position::new(1, 2));
    }

    #[test]
    fn test_incremental_edit_keeps_rest_of_document() {
        let mut text = String::from("host = \"localhost\"\nport = 8080\n");
        let start = position_to_offset(&text, Position::new(1, 0));
        let end = position_to_offset(&text, Position::new(1, 4));
        text.replace_range(start..end, "debug");
        assert_eq!(text, "host = \"localhost\"\ndebug = 8080\n");
    }

    #[test]
    fn test_parse_error_becomes_diagnostic_with_range() {
        let source = "ok = 1\nbad line here\n";
        let error = parse(source).unwrap_err();
        let diagnostic = diagnostic_from_error(&error, source);
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diagnostic.source.as_deref(), Some("verseconf"));
        assert_eq!(diagnostic.range.start.line, 1);
        assert!(diagnostic.message.contains("expected"));
    }

    #[test]
    fn test_valid_document_produces_no_diagnostics() {
        let source = "name = \"ok\"\nport = 8080\n";
        assert!(parse(source).is_ok());
    }
}
