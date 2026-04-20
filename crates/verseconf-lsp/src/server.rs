use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use verseconf_core::parse;
use std::collections::HashMap;
use std::sync::Mutex;

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
                semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
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
                )),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions::default(),
                )),
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
        self.documents.lock().unwrap().insert(uri.clone(), text.clone());
        self.validate_text(&uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.first() {
            let uri = params.text_document.uri.clone();
            let text = change.text.clone();
            self.documents.lock().unwrap().insert(uri.clone(), text.clone());
            self.validate_text(&uri, &text).await;
        }
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        self.client
            .log_message(MessageType::LOG, format!("completion at {:?}:{:?}", uri, position))
            .await;

        let mut items = vec![
            CompletionItem {
                label: "server".into(),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some("Server configuration block".into()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: "Configure server settings".into(),
                })),
                ..Default::default()
            },
            CompletionItem {
                label: "database".into(),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some("Database configuration block".into()),
                ..Default::default()
            },
            CompletionItem {
                label: "port".into(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some("Port number".into()),
                ..Default::default()
            },
            CompletionItem {
                label: "host".into(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some("Host address".into()),
                ..Default::default()
            },
        ];

        if let Some(doc) = self.documents.lock().unwrap().get(&uri) {
            let symbols = extract_symbols(doc);
            for symbol in symbols {
                items.push(CompletionItem {
                    label: symbol.name,
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some("Defined in document".into()),
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
            .log_message(MessageType::LOG, format!("hover at {:?}:{:?}", uri, position))
            .await;

        if let Some(doc) = self.documents.lock().unwrap().get(&uri) {
            if let Some(symbol) = find_symbol_at_position(doc, position) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("**{}**\n\nDefined in document", symbol.name),
                    }),
                    range: Some(symbol.range),
                }));
            }
        }

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "**VerseConf** configuration language\n\nA modern configuration language implementation.".into(),
            }),
            range: None,
        }))
    }

    async fn goto_definition(&self, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(MessageType::LOG, format!("goto definition at {:?}:{:?}", uri, position))
            .await;

        if let Some(doc) = self.documents.lock().unwrap().get(&uri) {
            if let Some(symbol) = find_symbol_at_position(doc, position) {
                if let Some(def_range) = find_symbol_definition(doc, &symbol.name) {
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location::new(uri, def_range))));
                }
            }
        }

        Ok(None)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        self.client
            .log_message(MessageType::LOG, format!("find references at {:?}:{:?}", uri, position))
            .await;

        if let Some(doc) = self.documents.lock().unwrap().get(&uri) {
            if let Some(symbol) = find_symbol_at_position(doc, position) {
                let locations = find_all_references(doc, &uri, &symbol.name);
                return Ok(Some(locations));
            }
        }

        Ok(None)
    }

    async fn document_symbol(&self, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        self.client
            .log_message(MessageType::LOG, format!("document symbols for {:?}", uri))
            .await;

        if let Some(doc) = self.documents.lock().unwrap().get(&uri) {
            let symbols = extract_symbols(doc);
            let lsp_symbols: Vec<SymbolInformation> = symbols
                .into_iter()
                .map(|s| SymbolInformation {
                    name: s.name,
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

    async fn semantic_tokens_full(&self, params: SemanticTokensParams) -> Result<Option<SemanticTokensResult>> {
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
}

struct SymbolInfo {
    name: String,
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
            if !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                let col_start = line.find(key).unwrap_or(0);
                symbols.push(SymbolInfo {
                    name: key.to_string(),
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
    symbols.into_iter().find(|s| {
        s.range.start.line <= position.line && s.range.end.line >= position.line
    })
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
                
                let is_word_boundary = 
                    (before.is_empty() || !before.chars().last().unwrap().is_alphanumeric()) &&
                    (after.is_empty() || !after.chars().next().unwrap().is_alphanumeric());

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
                let delta_start = if delta_line == 0 { start as u32 - prev_start } else { start as u32 };
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
                let delta_start = if delta_line == 0 { key_start as u32 - prev_start } else { key_start as u32 };
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
                    let delta_start = if delta_line == 0 { abs_val_start as u32 - prev_start } else { abs_val_start as u32 };
                    
                    let token_type = if value.starts_with('"') { 1 } else if value.chars().all(|c| c.is_numeric() || c == '.') { 2 } else { 0 };
                    
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

impl VerseConfBackend {
    pub fn new(client: Client) -> Self {
        Self { 
            client,
            documents: Mutex::new(HashMap::new()),
        }
    }

    async fn validate_text(&self, uri: &Url, text: &str) {
        match parse(text) {
            Ok(_) => {
                self.client
                    .publish_diagnostics(uri.clone(), vec![], None)
                    .await;
            }
            Err(e) => {
                let diagnostics = vec![Diagnostic::new_simple(
                    Range::new(Position::new(0, 0), Position::new(0, 1)),
                    format!("Parse error: {}", e),
                )];

                self.client
                    .publish_diagnostics(uri.clone(), diagnostics, None)
                    .await;
            }
        }
    }
}

pub async fn run() {
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(VerseConfBackend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
