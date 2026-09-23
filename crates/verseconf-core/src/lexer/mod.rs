pub mod context;
pub mod error;
pub mod token;

pub use context::*;
pub use error::*;
pub use token::*;

use crate::source::Span;

/// 词法分析器
pub struct Lexer<'a> {
    source: &'a str,
    position: usize,
    line: u32,
    column: u32,
    context: ContextStateMachine,
}

impl<'a> Lexer<'a> {
    /// 创建新的词法分析器
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            position: 0,
            line: 1,
            column: 1,
            context: ContextStateMachine::new(),
        }
    }

    /// 获取当前上下文
    pub fn context(&self) -> &ContextStateMachine {
        &self.context
    }

    /// 获取当前上下文的可变引用
    pub fn context_mut(&mut self) -> &mut ContextStateMachine {
        &mut self.context
    }

    /// 返回下一个 Token
    pub fn next_token(&mut self) -> Result<Token, LexError> {
        if self.position >= self.source.len() {
            return Ok(Token::Eof);
        }

        let ch = self.current_char();

        match ch {
            // 空白字符
            ' ' | '\t' => {
                self.advance();
                self.next_token()
            }
            // 换行（LF）
            '\n' => {
                self.advance();
                self.line += 1;
                self.column = 1;
                Ok(Token::Newline)
            }
            // 换行（CRLF / 孤立 CR）——Windows 检出与记事本文件同样可用
            '\r' => {
                self.advance();
                if self.position < self.source.len() && self.current_char() == '\n' {
                    self.advance();
                }
                self.line += 1;
                self.column = 1;
                Ok(Token::Newline)
            }
            // 注释
            '#' => {
                // Check for #@ metadata prefix
                if self.peek_char() == Some('@') {
                    self.advance(); // skip '#'
                    self.advance(); // skip '@'
                    Ok(Token::MetadataPrefix)
                } else {
                    self.read_comment()
                }
            }
            // @include 指令
            '@' => {
                let at_pos = self.position;
                let at_line = self.line;
                let at_column = self.column;
                self.advance(); // skip '@'
                let mut word = String::new();
                while self.position < self.source.len() {
                    let ch = self.current_char();
                    if ch.is_ascii_alphanumeric() || ch == '_' {
                        word.push(ch);
                        self.advance();
                    } else {
                        break;
                    }
                }
                match word.as_str() {
                    "include" => Ok(Token::Include),
                    _ => Err(LexError::new(
                        format!("unknown directive: @{}", word),
                        Span::at(at_pos, self.position, at_line, at_column),
                    )),
                }
            }
            // 字符串
            '"' => self.read_string(),
            // 大括号
            '{' => {
                self.advance();
                Ok(Token::LBrace)
            }
            '}' => {
                self.advance();
                Ok(Token::RBrace)
            }
            // 中括号
            '[' => self.read_bracket(),
            ']' => {
                self.advance();
                if self.position < self.source.len() && self.current_char() == ']' {
                    self.advance();
                    Ok(Token::RDoubleBracket)
                } else {
                    Ok(Token::RBracket)
                }
            }
            // 括号
            '(' => {
                self.advance();
                Ok(Token::LParen)
            }
            ')' => {
                self.advance();
                Ok(Token::RParen)
            }
            // 赋值符
            '=' => {
                self.advance();
                self.context.push(LexerContext::InValue);
                Ok(Token::Assign)
            }
            ':' => {
                self.advance();
                self.context.push(LexerContext::InValue);
                Ok(Token::Assign)
            }
            // 逗号
            ',' => {
                self.advance();
                Ok(Token::Comma)
            }
            // 运算符 + - * /
            '+' => {
                self.advance();
                Ok(Token::Operator(Operator::Add))
            }
            '-' => {
                if self.context.is_in_key() {
                    // In key context, '-' is part of BARE_KEY
                    self.read_bare_key()
                } else {
                    self.advance();
                    Ok(Token::Operator(Operator::Subtract))
                }
            }
            '*' => {
                self.advance();
                Ok(Token::Operator(Operator::Multiply))
            }
            '/' if self.peek_char() == Some('*') => self.read_block_comment(),
            '/' => {
                self.advance();
                Ok(Token::Operator(Operator::Divide))
            }
            // 点 / 范围运算符
            '.' => {
                self.advance();
                if self.position < self.source.len() && self.current_char() == '.' {
                    self.advance();
                    Ok(Token::RangeOp)
                } else {
                    Ok(Token::Dot)
                }
            }
            // 数字
            '0'..='9' => self.read_number(),
            // 裸键
            'a'..='z' | 'A'..='Z' | '_' => {
                if self.context.is_in_key() {
                    self.read_bare_key()
                } else {
                    self.read_identifier_or_keyword()
                }
            }
            // 元数据前缀 #@
            _ if self.peek_char() == Some('@') && ch == '#' => {
                self.advance();
                self.advance();
                Ok(Token::MetadataPrefix)
            }
            _ => Err(LexError::new(
                format!("unexpected character: '{}'", ch),
                Span::at(
                    self.position,
                    self.position + ch.len_utf8(),
                    self.line,
                    self.column,
                ),
            )),
        }
    }

    /// 返回所有 Tokens
    pub fn tokenize_all(&mut self) -> Result<Vec<(Token, Span)>, LexError> {
        let mut tokens = Vec::new();
        loop {
            let (token, span) = self.next_token_spanned()?;
            let is_eof = token == Token::Eof;
            tokens.push((token, span));
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    /// 返回下一个 Token 及其准确位置（跳过行内空白后再取起始位置）
    pub fn next_token_spanned(&mut self) -> Result<(Token, Span), LexError> {
        while self.position < self.source.len() {
            match self.current_char() {
                ' ' | '\t' => self.advance(),
                _ => break,
            }
        }

        let start = self.position;
        let line = self.line;
        let column = self.column;
        let token = self.next_token()?;
        Ok((token, Span::at(start, self.position, line, column)))
    }

    fn current_char(&self) -> char {
        self.source[self.position..].chars().next().unwrap()
    }

    fn peek_char(&self) -> Option<char> {
        let next = self.position + self.current_char().len_utf8();
        if next < self.source.len() {
            Some(self.source[next..].chars().next().unwrap())
        } else {
            None
        }
    }

    fn advance(&mut self) {
        if self.position < self.source.len() {
            let ch = self.current_char();
            self.position += ch.len_utf8();
            self.column += 1;
        }
    }

    fn read_comment(&mut self) -> Result<Token, LexError> {
        self.advance(); // skip '#'

        let mut content = String::new();
        while self.position < self.source.len()
            && self.current_char() != '\n'
            && self.current_char() != '\r'
        {
            content.push(self.current_char());
            self.advance();
        }
        Ok(Token::LineComment(content.trim().to_string()))
    }

    fn read_block_comment(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        let start_line = self.line;
        let start_column = self.column;
        self.advance(); // skip '/'
        self.advance(); // skip '*'

        let mut content = String::new();
        while self.position < self.source.len() {
            if self.current_char() == '*' && self.peek_char() == Some('/') {
                self.advance();
                self.advance();
                return Ok(Token::BlockComment(content));
            }

            match self.current_char() {
                '\n' => {
                    content.push('\n');
                    self.advance();
                    self.line += 1;
                    self.column = 1;
                }
                '\r' => {
                    content.push('\r');
                    self.advance();
                    if self.position < self.source.len() && self.current_char() == '\n' {
                        content.push('\n');
                        self.advance();
                    }
                    self.line += 1;
                    self.column = 1;
                }
                ch => {
                    content.push(ch);
                    self.advance();
                }
            }
        }

        Err(LexError::new(
            "unterminated block comment",
            Span::at(start, self.position, start_line, start_column),
        ))
    }

    fn read_string(&mut self) -> Result<Token, LexError> {
        let start = self.position;
        let start_line = self.line;
        let start_column = self.column;
        self.advance(); // skip opening quote

        let mut content = String::new();
        while self.position < self.source.len() {
            let ch = self.current_char();
            if ch == '"' {
                self.advance();
                return Ok(Token::StringLiteral(content));
            }
            if ch == '\\' {
                self.advance();
                if self.position < self.source.len() {
                    let escaped = self.current_char();
                    match escaped {
                        'n' => content.push('\n'),
                        't' => content.push('\t'),
                        'r' => content.push('\r'),
                        '\\' => content.push('\\'),
                        '"' => content.push('"'),
                        _ => {
                            content.push('\\');
                            content.push(escaped);
                        }
                    }
                    self.advance();
                }
            } else if ch == '\n' {
                content.push(ch);
                self.advance();
                self.line += 1;
                self.column = 1;
            } else if ch == '\r' {
                content.push(ch);
                self.advance();
                if self.position < self.source.len() && self.current_char() == '\n' {
                    content.push('\n');
                    self.advance();
                }
                self.line += 1;
                self.column = 1;
            } else {
                content.push(ch);
                self.advance();
            }
        }
        Err(LexError::new(
            "unterminated string",
            Span::at(start, self.position, start_line, start_column),
        ))
    }

    fn read_number(&mut self) -> Result<Token, LexError> {
        let _start = self.position;
        let mut number = String::new();
        let mut has_dot = false;

        while self.position < self.source.len() {
            let ch = self.current_char();
            if ch.is_ascii_digit() {
                number.push(ch);
                self.advance();
            } else if ch == '.' {
                if has_dot {
                    break; // Second dot, stop here
                }
                // Check if this is a range operator '..'
                if self.position + 1 < self.source.len()
                    && self.source.as_bytes()[self.position + 1] == b'.'
                {
                    break; // This is '..', stop here
                }
                has_dot = true;
                number.push(ch);
                self.advance();
            } else if ch == '-' && number.len() == 4 {
                // Could be start of datetime: YYYY-MM-DD...
                number.push(ch);
                self.advance();
                return self.read_datetime_rest(number);
            } else {
                break;
            }
        }

        // 检查是否是持续时间
        if self.position < self.source.len() {
            let ch = self.current_char();
            if ch == 's' || ch == 'm' || ch == 'h' || ch == 'd' {
                number.push(ch);
                self.advance();
                return Ok(Token::DurationLiteral(number));
            }
        }

        Ok(Token::NumberLiteral(number))
    }

    fn read_datetime_rest(&mut self, mut dt: String) -> Result<Token, LexError> {
        // Expect: MM-DDTHH:MM:SS[Z|+HH:MM|-HH:MM]
        // Already read YYYY-
        let mut parts_read = 0;

        // Read MM
        while self.position < self.source.len()
            && self.current_char().is_ascii_digit()
            && parts_read < 2
        {
            dt.push(self.current_char());
            self.advance();
            parts_read += 1;
        }

        if parts_read != 2 || self.position >= self.source.len() || self.current_char() != '-' {
            return Ok(Token::NumberLiteral(dt));
        }
        dt.push('-');
        self.advance();

        // Read DD
        parts_read = 0;
        while self.position < self.source.len()
            && self.current_char().is_ascii_digit()
            && parts_read < 2
        {
            dt.push(self.current_char());
            self.advance();
            parts_read += 1;
        }

        if parts_read != 2 || self.position >= self.source.len() {
            return Ok(Token::NumberLiteral(dt));
        }

        // Check for T separator
        if self.current_char() == 'T' || self.current_char() == 't' {
            dt.push(self.current_char());
            self.advance();

            // Read HH:MM:SS
            let time_parts = ["HH", "MM", "SS"];
            for (i, _) in time_parts.iter().enumerate() {
                let mut digits = 0;
                while self.position < self.source.len()
                    && self.current_char().is_ascii_digit()
                    && digits < 2
                {
                    dt.push(self.current_char());
                    self.advance();
                    digits += 1;
                }

                if digits == 0 {
                    return Ok(Token::NumberLiteral(dt));
                }

                // Add colon separator (except after last part)
                if i < 2 && self.position < self.source.len() && self.current_char() == ':' {
                    dt.push(':');
                    self.advance();
                }
            }

            // Read timezone
            if self.position < self.source.len() {
                let tz_ch = self.current_char();
                if tz_ch == 'Z' || tz_ch == 'z' {
                    dt.push(tz_ch);
                    self.advance();
                } else if tz_ch == '+' || tz_ch == '-' {
                    dt.push(tz_ch);
                    self.advance();
                    // Read HH:MM
                    let mut digits = 0;
                    while self.position < self.source.len()
                        && self.current_char().is_ascii_digit()
                        && digits < 2
                    {
                        dt.push(self.current_char());
                        self.advance();
                        digits += 1;
                    }
                    if self.position < self.source.len() && self.current_char() == ':' {
                        dt.push(':');
                        self.advance();
                        digits = 0;
                        while self.position < self.source.len()
                            && self.current_char().is_ascii_digit()
                            && digits < 2
                        {
                            dt.push(self.current_char());
                            self.advance();
                            digits += 1;
                        }
                    }
                }
            }
        }

        Ok(Token::DateTimeLiteral(dt))
    }

    fn read_bare_key(&mut self) -> Result<Token, LexError> {
        let _start = self.position;
        let mut key = String::new();

        while self.position < self.source.len() {
            let ch = self.current_char();
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                key.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        match key.as_str() {
            "merge" => Ok(Token::Merge),
            _ => Ok(Token::BareKey(key)),
        }
    }

    fn read_identifier_or_keyword(&mut self) -> Result<Token, LexError> {
        let _start = self.position;
        let mut identifier = String::new();

        while self.position < self.source.len() {
            let ch = self.current_char();
            if ch.is_ascii_alphanumeric() || ch == '_' {
                identifier.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        match identifier.as_str() {
            "true" => Ok(Token::BooleanLiteral(true)),
            "false" => Ok(Token::BooleanLiteral(false)),
            "@include" => Ok(Token::Include),
            "merge" => Ok(Token::Merge),
            _ => Ok(Token::BareKey(identifier)),
        }
    }

    fn read_bracket(&mut self) -> Result<Token, LexError> {
        self.advance(); // skip '['

        if self.position < self.source.len() && self.current_char() == '[' {
            self.advance();
            Ok(Token::LDoubleBracket)
        } else {
            Ok(Token::LBracket)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_basic() {
        let mut lexer = Lexer::new("name = \"test\"");
        let tokens = lexer.tokenize_all().unwrap();
        assert!(tokens.iter().any(|(t, _)| matches!(t, Token::BareKey(_))));
        assert!(tokens.iter().any(|(t, _)| matches!(t, Token::Assign)));
        assert!(tokens
            .iter()
            .any(|(t, _)| matches!(t, Token::StringLiteral(_))));
    }

    #[test]
    fn test_lexer_context_sensitive() {
        let mut lexer = Lexer::new("my-key = 10");
        lexer.context_mut().push(LexerContext::InKey);
        let token = lexer.next_token().unwrap();
        assert!(matches!(token, Token::BareKey(_)));
    }

    #[test]
    fn test_lexer_accepts_crlf_line_endings() {
        let mut lexer = Lexer::new("a = 1\r\nb = 2\r\n");
        let tokens = lexer.tokenize_all().unwrap();
        assert!(tokens.iter().all(|(t, _)| !matches!(t, Token::Eof) || true));
        let newlines = tokens
            .iter()
            .filter(|(t, _)| matches!(t, Token::Newline))
            .count();
        assert_eq!(
            newlines, 2,
            "CRLF should produce exactly two newline tokens"
        );
        assert!(tokens
            .iter()
            .any(|(t, _)| matches!(t, Token::BareKey(k) if k == "b")));
    }

    #[test]
    fn test_lexer_accepts_lone_cr_line_endings() {
        let mut lexer = Lexer::new("a = 1\rb = 2\r");
        let tokens = lexer.tokenize_all().unwrap();
        let newlines = tokens
            .iter()
            .filter(|(t, _)| matches!(t, Token::Newline))
            .count();
        assert_eq!(newlines, 2);
    }

    #[test]
    fn test_lexer_crlf_tracks_line_numbers() {
        let mut lexer = Lexer::new("a = 1\r\nb = 2\r\n");
        let tokens = lexer.tokenize_all().unwrap();
        let b_span = tokens
            .iter()
            .find_map(|(t, s)| match t {
                Token::BareKey(k) if k == "b" => Some(*s),
                _ => None,
            })
            .expect("token 'b' should exist");
        assert_eq!(b_span.line, 2);
        assert_eq!(b_span.column, 1);
    }

    #[test]
    fn test_lexer_error_reports_location() {
        let mut lexer = Lexer::new("a = 1\nb = $\n");
        let err = lexer.tokenize_all().unwrap_err();
        assert_eq!(err.span.line, 2);
        assert_eq!(err.span.column, 5);
    }

    #[test]
    fn test_lexer_reads_block_comments() {
        let mut lexer = Lexer::new("a = 1 /* note\ncontinued */\nb = 2");
        let tokens = lexer.tokenize_all().unwrap();
        let comment = tokens
            .iter()
            .find_map(|(token, span)| match token {
                Token::BlockComment(content) => Some((content.as_str(), *span)),
                _ => None,
            })
            .expect("block comment should be tokenized");
        assert_eq!(comment.0, " note\ncontinued ");
        assert_eq!(comment.1.line, 1);
        assert_eq!(comment.1.column, 7);
        assert!(tokens.iter().any(
            |(token, span)| matches!(token, Token::BareKey(key) if key == "b") && span.line == 3
        ));
    }

    #[test]
    fn test_lexer_rejects_unterminated_block_comment() {
        let mut lexer = Lexer::new("value = 1 /* missing close");
        let error = lexer.tokenize_all().unwrap_err();
        assert!(error.message.contains("unterminated block comment"));
        assert_eq!(error.span.line, 1);
        assert_eq!(error.span.column, 11);
    }

    #[test]
    fn test_lexer_crlf_comment_not_including_carriage_return() {
        let mut lexer = Lexer::new("# hello\r\n");
        let tokens = lexer.tokenize_all().unwrap();
        let comment = tokens
            .iter()
            .find_map(|(t, _)| match t {
                Token::LineComment(c) => Some(c.clone()),
                _ => None,
            })
            .expect("comment token");
        assert_eq!(comment, "hello");
    }
}
