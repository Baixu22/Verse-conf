/// Lexer 上下文状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexerContext {
    /// 键上下文: `-` 属于 BARE_KEY
    InKey,
    /// 值上下文: `-` 属于 OPERATOR
    InValue,
    /// 表块内部（不允许逗号）
    InTableBlock,
    /// 内联表内部（必须逗号）
    InInlineTable,
}

/// 上下文状态机
pub struct ContextStateMachine {
    current: LexerContext,
    pub stack: Vec<LexerContext>,
}

impl ContextStateMachine {
    /// 创建新的状态机，默认处于 InKey 状态
    pub fn new() -> Self {
        Self {
            current: LexerContext::InKey,
            stack: Vec::new(),
        }
    }

    /// 压入新上下文
    pub fn push(&mut self, ctx: LexerContext) {
        self.stack.push(self.current);
        self.current = ctx;
    }

    /// 弹出上下文
    pub fn pop(&mut self) {
        if let Some(prev) = self.stack.pop() {
            self.current = prev;
        }
    }

    /// 获取当前上下文
    pub fn current(&self) -> LexerContext {
        self.current
    }

    /// 是否在键上下文
    pub fn is_in_key(&self) -> bool {
        self.current == LexerContext::InKey
    }

    /// 是否在值上下文
    pub fn is_in_value(&self) -> bool {
        self.current == LexerContext::InValue
    }

    /// 是否在表块内部
    pub fn is_in_table_block(&self) -> bool {
        self.current == LexerContext::InTableBlock
    }

    /// 是否在内联表内部
    pub fn is_in_inline_table(&self) -> bool {
        self.current == LexerContext::InInlineTable
    }
}

impl Default for ContextStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_push_pop() {
        let mut ctx = ContextStateMachine::new();
        assert!(ctx.is_in_key());

        ctx.push(LexerContext::InValue);
        assert!(ctx.is_in_value());

        ctx.pop();
        assert!(ctx.is_in_key());
    }

    #[test]
    fn test_context_nested() {
        let mut ctx = ContextStateMachine::new();
        ctx.push(LexerContext::InTableBlock);
        ctx.push(LexerContext::InValue);
        assert!(ctx.is_in_value());

        ctx.pop();
        assert!(ctx.is_in_table_block());

        ctx.pop();
        assert!(ctx.is_in_key());
    }
}
