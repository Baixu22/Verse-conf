use std::fmt;
use std::time::Duration as StdDuration;

/// 标量值
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue {
    String(String),
    Number(NumberValue),
    Boolean(bool),
    DateTime(String),
    Duration(StdDuration),
}

impl fmt::Display for ScalarValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScalarValue::String(s) => write!(f, "\"{}\"", s),
            ScalarValue::Number(n) => write!(f, "{}", n),
            ScalarValue::Boolean(b) => write!(f, "{}", b),
            ScalarValue::DateTime(dt) => write!(f, "{}", dt),
            ScalarValue::Duration(d) => write!(f, "{:?}", d),
        }
    }
}

/// 数字值（保留原始精度信息）
#[derive(Debug, Clone, PartialEq)]
pub enum NumberValue {
    Integer(i64),
    Float(f64),
}

impl fmt::Display for NumberValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NumberValue::Integer(n) => write!(f, "{}", n),
            NumberValue::Float(n) => write!(f, "{}", n),
        }
    }
}

impl NumberValue {
    /// 转换为 f64
    pub fn as_f64(&self) -> f64 {
        match self {
            NumberValue::Integer(n) => *n as f64,
            NumberValue::Float(n) => *n,
        }
    }
}

/// 二元运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl fmt::Display for BinaryOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinaryOperator::Add => write!(f, "+"),
            BinaryOperator::Subtract => write!(f, "-"),
            BinaryOperator::Multiply => write!(f, "*"),
            BinaryOperator::Divide => write!(f, "/"),
        }
    }
}

/// 时间单位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
}

impl TimeUnit {
    /// 转换为秒数
    pub fn to_seconds(&self) -> u64 {
        match self {
            TimeUnit::Seconds => 1,
            TimeUnit::Minutes => 60,
            TimeUnit::Hours => 3600,
            TimeUnit::Days => 86400,
        }
    }
}

/// 表达式（解析时求值）
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    BinaryOp {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
    },
    Literal(ScalarValue),
    UnitValue {
        value: f64,
        unit: TimeUnit,
    },
}

impl Expression {
    /// 求值表达式（简化版本，仅支持基础运算）
    pub fn evaluate(&self) -> Result<ScalarValue, ExpressionError> {
        match self {
            Expression::Literal(val) => Ok(val.clone()),
            Expression::UnitValue { value, unit } => {
                let seconds = (*value as u64) * unit.to_seconds();
                Ok(ScalarValue::Duration(StdDuration::from_secs(seconds)))
            }
            Expression::BinaryOp {
                left,
                operator,
                right,
            } => {
                let l = left.evaluate()?;
                let r = right.evaluate()?;
                apply_operator(&l, operator, &r)
            }
        }
    }
}

fn apply_operator(
    left: &ScalarValue,
    op: &BinaryOperator,
    right: &ScalarValue,
) -> Result<ScalarValue, ExpressionError> {
    match (left, right) {
        (ScalarValue::Number(l), ScalarValue::Number(r)) => {
            let result = match op {
                BinaryOperator::Add => l.as_f64() + r.as_f64(),
                BinaryOperator::Subtract => l.as_f64() - r.as_f64(),
                BinaryOperator::Multiply => l.as_f64() * r.as_f64(),
                BinaryOperator::Divide => {
                    if r.as_f64() == 0.0 {
                        return Err(ExpressionError::DivisionByZero);
                    }
                    l.as_f64() / r.as_f64()
                }
            };
            Ok(ScalarValue::Number(NumberValue::Float(result)))
        }
        _ => Err(ExpressionError::TypeMismatch),
    }
}

/// 表达式错误
#[derive(Debug, thiserror::Error)]
pub enum ExpressionError {
    #[error("division by zero")]
    DivisionByZero,
    #[error("type mismatch in expression")]
    TypeMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expression_evaluate_literal() {
        let expr = Expression::Literal(ScalarValue::Number(NumberValue::Integer(42)));
        assert_eq!(
            expr.evaluate().unwrap(),
            ScalarValue::Number(NumberValue::Integer(42))
        );
    }

    #[test]
    fn test_expression_evaluate_binary() {
        let expr = Expression::BinaryOp {
            left: Box::new(Expression::Literal(ScalarValue::Number(NumberValue::Integer(10)))),
            operator: BinaryOperator::Add,
            right: Box::new(Expression::Literal(ScalarValue::Number(NumberValue::Integer(5)))),
        };
        assert_eq!(
            expr.evaluate().unwrap(),
            ScalarValue::Number(NumberValue::Float(15.0))
        );
    }

    #[test]
    fn test_time_unit_to_seconds() {
        assert_eq!(TimeUnit::Minutes.to_seconds(), 60);
        assert_eq!(TimeUnit::Hours.to_seconds(), 3600);
    }
}
