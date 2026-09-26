use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use verseconf_core::{parse, IndentStyle, PrettyPrintConfig, VerseconfError};

pub struct VerseConfBackend {
    client: Client,
    documents: Mutex<HashMap<Url, String>>,
    /// 是否已经收到过 `shutdown`。决定 `exit` 之后的退出码：
    /// 先 shutdown 再 exit → 0；未经 shutdown 直接 exit → 1（LSP 规范要求）。
    shutdown_seen: Arc<AtomicBool>,
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
                // 文档格式化：编辑器里的 `verseconf.format` 命令依赖它。
                // 此前这里没有声明，扩展把命令转发给 editor.action.formatDocument
                // 却找不到任何 provider，于是"格式化"永远没有任何效果。
                document_formatting_provider: Some(OneOf::Left(true)),
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
        // 记下来，供 exit 监护任务决定退出码。
        self.shutdown_seen.store(true, Ordering::SeqCst);
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

        // 没有命中任何符号时返回 None。
        // 之前这里无条件回一张"VerseConf 快速参考"样板卡，导致畸形行、空白处、
        // 甚至从未打开过的文档都返回同一份内容，客户端无法区分
        // "此处无信息" 与 "此处有信息"。
        Ok(None)
    }

    /// 文档格式化。
    ///
    /// 复用核心库的保注释格式化器：注释与 `#@` 元数据都会原样保留，
    /// 这也是本项目"改动之外字节不变"这条主张在编辑器里的延伸。
    ///
    /// 两种情况下**不做任何修改**，而不是给出一个猜出来的结果：
    /// - 文档无法解析（语法错误时格式化只会把错误放大）；
    /// - 格式化结果与原文一致（返回空编辑列表，让编辑器保持原样）。
    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;

        let text = match self.documents.lock().unwrap().get(&uri).cloned() {
            Some(text) => text,
            None => return Ok(None),
        };

        Ok(formatting_edits(&text, &params.options))
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
    symbols.into_iter().find(|s| {
        // 只比对行号会让整行任意列都命中同一个符号：光标停在行尾空白处
        // 也会返回该行的符号信息。列号必须一起判定。
        if position.line < s.range.start.line || position.line > s.range.end.line {
            return false;
        }
        if s.range.start.line == s.range.end.line {
            return position.character >= s.range.start.character
                && position.character <= s.range.end.character;
        }
        // 跨行符号：首行只接受起始列之后，末行只接受结束列之前
        if position.line == s.range.start.line {
            return position.character >= s.range.start.character;
        }
        if position.line == s.range.end.line {
            return position.character <= s.range.end.character;
        }
        true
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

/// `verseconf/generateSchema` 的请求参数。
///
/// 直接传文本而不是 URI：这样结果只取决于调用方手里的那份内容，
/// 不受服务端文档缓存与消息先后顺序影响。
#[derive(Debug, serde::Deserialize)]
pub struct GenerateSchemaParams {
    pub text: String,
}

/// `verseconf/generateSchema` 的响应
#[derive(Debug, serde::Serialize)]
pub struct GenerateSchemaResponse {
    /// 可直接写进配置文件的 `#@schema { ... }` 文本
    pub schema: String,
}

impl VerseConfBackend {
    /// 自定义请求：由一份配置文本推断出 `#@schema { ... }` 块。
    ///
    /// 走语言服务器而不是让扩展另找命令行，是为了保持"一个二进制、一条通道"：
    /// 扩展包里已经带着这个服务端，不需要用户再装别的东西。
    pub async fn generate_schema(
        &self,
        params: GenerateSchemaParams,
    ) -> tower_lsp::jsonrpc::Result<GenerateSchemaResponse> {
        match verseconf_core::infer_schema(&params.text) {
            Ok(schema) => Ok(GenerateSchemaResponse { schema }),
            // 解析不了就如实报错，不返回半份 schema
            Err(error) => Err(tower_lsp::jsonrpc::Error::invalid_params(error.to_string())),
        }
    }

    pub fn new(client: Client) -> Self {
        Self::with_shutdown_flag(client, Arc::new(AtomicBool::new(false)))
    }

    /// 构造后端并复用外部的 shutdown 标志，使 `exit` 监护任务能读到它。
    pub fn with_shutdown_flag(client: Client, shutdown_seen: Arc<AtomicBool>) -> Self {
        Self {
            client,
            documents: Mutex::new(HashMap::new()),
            shutdown_seen,
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

/// 计算格式化需要的编辑列表。
///
/// 抽成纯函数是为了能直接单测：`formatting()` 需要 LSP `Client`，
/// 而这里只依赖文档文本与编辑器选项。
///
/// 返回 `None` 表示"什么都不做"——文档无法解析时绝不给出猜出来的结果。
fn formatting_edits(text: &str, options: &FormattingOptions) -> Option<Vec<TextEdit>> {
    // 跟随编辑器自己的缩进设置，避免格式化结果与用户的编辑器配置互相打架
    let config = PrettyPrintConfig {
        indent_size: options.tab_size.max(1) as usize,
        indent_style: if options.insert_spaces {
            IndentStyle::Spaces
        } else {
            IndentStyle::Tabs
        },
        ..PrettyPrintConfig::default()
    };

    match verseconf_core::format_with_config(text, config) {
        Ok(formatted) if formatted != text => {
            let end = offset_to_position(text, text.len());
            Some(vec![TextEdit {
                range: Range::new(Position::new(0, 0), end),
                new_text: formatted,
            }])
        }
        // 已经规范：返回空编辑列表，让编辑器保持原样
        Ok(_) => Some(Vec::new()),
        // 解析失败：保持沉默
        Err(_) => None,
    }
}

/// 把带位置的解析错误转成编辑器诊断
fn diagnostic_from_error(error: &VerseconfError, text: &str) -> Diagnostic {
    let span = error.span();

    let range = if span.is_unknown() {
        Range::new(Position::new(0, 0), Position::new(0, 0))
    } else {
        // 起点必须和终点走同一套换算：span.column 是 Unicode 标量计数的列号，
        // 而 LSP 的 character 以 UTF-16 code unit 计。非 BMP 字符（emoji 等）
        // 会让两者差 1，之前的实现让 START 用字符计数、END 用 UTF-16，
        // 同一个 range 内部自相矛盾。
        let start = if span.start < text.len() {
            offset_to_position(text, span.start)
        } else {
            Position::new(span.line.saturating_sub(1), span.column.saturating_sub(1))
        };
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

    let shutdown_seen = Arc::new(AtomicBool::new(false));
    let (exit_tx, exit_rx) = tokio::sync::mpsc::unbounded_channel();

    // serve() 在收到 exit 之后可能永不返回（见 exit_watcher 模块头的死锁说明），
    // 所以退出必须由一个独立任务从 serve() 之外驱动。
    crate::exit_watcher::spawn_exit_watcher(shutdown_seen.clone(), exit_rx);

    let stdin = crate::exit_watcher::ExitAwareStdin::new(tokio::io::stdin(), exit_tx);
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(move |client| {
        VerseConfBackend::with_shutdown_flag(client, shutdown_seen)
    })
    .custom_method(
        "verseconf/generateSchema",
        VerseConfBackend::generate_schema,
    )
    .finish();
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

    #[test]
    fn test_diagnostic_start_column_uses_utf16_like_the_end() {
        // 回归：START 曾经用 Unicode 标量列号、END 用 UTF-16，同一个 range
        // 内部不自洽。非 BMP 字符（emoji）会让两者差 1。
        let ascii = "x = \"ab\" bad line here\n";
        let ascii_diag = diagnostic_from_error(&parse(ascii).unwrap_err(), ascii);

        let astral = "x = \"😀\" bad line here\n";
        let astral_diag = diagnostic_from_error(&parse(astral).unwrap_err(), astral);

        // 两段文本里 "bad" 的 UTF-16 偏移都是 13：emoji 占 2 个 code unit，
        // 与 "ab" 的 2 个字符宽度相同，所以两边的列号必须相等。
        assert_eq!(
            ascii_diag.range.start.character, astral_diag.range.start.character,
            "ASCII 与 astral 文档里同一处错误的起始列必须一致"
        );
        assert_eq!(ascii_diag.range.start.character, 13);
        assert_eq!(
            ascii_diag.range.end.character,
            astral_diag.range.end.character
        );
    }

    #[test]
    fn test_find_symbol_at_position_respects_the_column() {
        // 回归：只比对行号时，整行任意列都命中同一个符号。
        let text = "name = \"ok\"\n";
        assert!(find_symbol_at_position(text, Position::new(0, 0)).is_some());
        assert!(find_symbol_at_position(text, Position::new(0, 2)).is_some());
        // 行尾空白处（列 50）不该命中该符号
        assert!(
            find_symbol_at_position(text, Position::new(0, 50)).is_none(),
            "光标在行尾空白处不应返回该行符号"
        );
    }

    #[test]
    fn test_hover_source_has_no_boilerplate_fallback() {
        // 回归：无命中时必须返回 None，让客户端能区分"无信息"与"有信息"。
        // 之前无条件回一张样板卡，畸形行与从未打开的文档都拿到相同内容。
        let source = include_str!("server.rs");
        let hover_body = source
            .split("async fn hover")
            .nth(1)
            .expect("server.rs 里应当有 hover 实现");
        let hover_body = hover_body.split("async fn formatting").next().unwrap();
        assert!(
            hover_body.contains("Ok(None)"),
            "hover 无命中时必须返回 None"
        );
        assert!(
            !hover_body.contains("A modern configuration language for the AI era"),
            "hover 里不应再出现样板卡文案"
        );
    }

    fn default_format_options() -> FormattingOptions {
        FormattingOptions {
            tab_size: 2,
            insert_spaces: true,
            ..FormattingOptions::default()
        }
    }

    #[test]
    fn test_formatting_preserves_comments_and_metadata() {
        // 格式化必须保住注释与 #@ 元数据——这是本项目在编辑器里的核心主张
        let messy = "server {
      host = \"127.0.0.1\"   # 监听地址
   port = 8080  #@ range(1024..65535)
}
";
        let edits =
            formatting_edits(messy, &default_format_options()).expect("可解析的文档应当能格式化");
        assert_eq!(edits.len(), 1, "应当只返回一个整篇替换编辑");

        let formatted = &edits[0].new_text;
        assert!(formatted.contains("# 监听地址"), "行尾注释必须保留");
        assert!(
            formatted.contains("#@ range(1024..65535)"),
            "#@ 元数据必须保留"
        );
        assert!(formatted.contains("host = \"127.0.0.1\""), "值不能被改掉");
    }

    #[test]
    fn test_formatting_is_idempotent() {
        let messy = "server {
      host = \"127.0.0.1\"   # 注释
}
";
        let first = formatting_edits(messy, &default_format_options()).unwrap();
        assert_eq!(first.len(), 1);
        let once = first[0].new_text.clone();

        // 再格式化一次不应产生任何编辑
        let second = formatting_edits(&once, &default_format_options()).unwrap();
        assert!(
            second.is_empty(),
            "格式化必须幂等：第二次不应再产生编辑，实际 {:?}",
            second
        );
    }

    #[test]
    fn test_formatting_refuses_unparseable_documents() {
        // 语法错误时保持沉默，而不是把坏文档改成另一个样子
        let broken = "bad line here
";
        assert!(
            formatting_edits(broken, &default_format_options()).is_none(),
            "无法解析的文档必须返回 None（不做任何修改）"
        );
    }

    #[test]
    fn test_formatting_follows_editor_indent_settings() {
        let source = "server {
  host = \"x\"
}
";
        let four_spaces = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            ..FormattingOptions::default()
        };
        let edits = formatting_edits(source, &four_spaces).expect("应当能格式化");
        assert_eq!(edits.len(), 1, "缩进变化应当产生编辑");
        assert!(
            edits[0].new_text.contains("    host = \"x\""),
            "应当使用编辑器设置的 4 空格缩进，实际：{:?}",
            edits[0].new_text
        );
    }

    #[test]
    fn test_formatting_covers_the_whole_document() {
        let source = "a = 1
b = 2
";
        let edits = formatting_edits(source, &default_format_options()).unwrap();
        if !edits.is_empty() {
            assert_eq!(edits[0].range.start, Position::new(0, 0));
        }
        // 已规范或需替换，两种情况都不允许出现"部分覆盖"的编辑
        assert!(edits.len() <= 1, "不应当返回多个编辑");
    }
}
