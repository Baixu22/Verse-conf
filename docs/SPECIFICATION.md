# VerseConf Language Specification v1.5（目标规范）

> **一句话**：本文档描述 VerseConf 的**目标语言**；当前实现 `verseconf 0.2.0` 只覆盖其中一个子集。
> **带 🚧 的条目照抄进 `.vcf` 文件会被 CLI 拒绝**（`parse` 退出码 1）。
> 想直接写出能跑通的配置，请看 [0.5 最小可用样例](#05-最小可用样例实测通过) 与 [A.2 实测可解析的完整示例](#a2-实测可解析的完整示例)。

---

## 0. 本文档的状态说明

### 0.1 为什么需要这一节

本文件此前把"设计目标"和"已实现能力"混在一起叙述，全文没有任何状态标注；
而 README 又声称"标注为未实现的条目请以本节为准"——指向了一份并不存在的标注。
本版本补齐标注，并把实测依据写在明处：**每一条状态都能用一条命令复现**（见 [附录 D](#附录-d-实测记录)）。

### 0.2 状态标记

| 标记 | 含义 | 判定依据 |
|------|------|----------|
| ✅ **已实现** | 该写法被当前实现接受 | `verseconf parse` 退出码 0 |
| 🚧 **未实现** | 该写法被当前实现拒绝 | `verseconf parse` 退出码 1（附实测错误） |
| ⚠️ **部分实现** | 同一特性只有部分写法可用 | 逐条实测，见 [附录 D](#附录-d-实测记录) |
| ❓ **未验证** | 未做实测，**不得**当作已实现使用 | —— |

"未实现"只描述**当前版本的行为**，不代表设计被否决：这些条目是路线图，不是现状。

### 0.3 版本对齐（消除 v1.5 / v1.1 / 0.2.0 三处漂移）

仓库里同时存在三个版本号，含义各不相同：

| 出现位置 | 值 | 含义 |
|----------|-----|------|
| 本文件标题、页脚 | `1.5` | **目标语言规范**版本（本文档描述的愿望清单） |
| `Cargo.toml` workspace `description` | `based on VerseConf v1.1 spec` | 实现**自述**的规范基线 |
| `Cargo.toml` 的 `[workspace.package] version` / `verseconf --version` | `0.2.0` | **软件**版本（实测输出 `verseconf 0.2.0`） |
| `#@schema { version = "1.0" }` 中的 `version` | `1.0` | **schema 版本**，与语言版本无关 |

**结论**：`v1.5` 是目标，`0.2.0` 是实现，两者不是一回事。
判断某个写法能不能用，**只看状态标记，不看版本号**。

### 0.4 一页速查

**✅ 已实现**（可放心使用）

- `#` 单行注释、行尾注释
- 裸键、双引号字符串、`true` / `false`
- 正整数、正浮点数字面量
- 日期 `2024-06-01`、时间戳 `2024-01-15T09:00:00Z`
- 时长 `30s` / `30m` / `1h`
- 数组（含混合类型、尾逗号）、块表、表数组、内联表 `{ a = 1 }`
- **引号内**表达式（`+ - * /`、变量引用、`${ENV:VAR}`、默认值 `:` / `|` / `or`）
- `@include "x.vcf"`、`merge=`、`if="..."`
- `#@experimental`、`#@sensitive`、`#@use`（仅语法层面）
- `#@schema { ... }` 块；`validate` 的 `range` / `required` / `enum` 校验；`validate --strict`
- CRLF 行尾、UTF-8 文本

**🚧 未实现**（照抄会报错）

- `###` 块注释
- 带引号的键：`"server.host"`、`'key-with-dashes'`
- 三引号字符串、`r"..."` 原始字符串
- 负数字面量 `-42`、科学计数法 `1.5e-10` / `1.5e10`、`0xFF5733`、`0b1010_1111`
- 时间字面量 `02:30:00`
- 毫秒时长 `500ms`、复合时长 `1h30m`
- **裸**表达式 `${...}`（任何位置，包括赋值右侧和表内）
- 表达式内嵌套引号（`"${"/" + prefix + "/v1"}"`）、比较运算符 `==`
- `@include "x.vcf" if=${ENV:LOCAL_DEV}`（裸形式）
- `#@template` 与模板继承
- `#@deprecated "msg"`、`#@llm_hint "msg"`
- UTF-8 BOM 文件头
- schema 的 `pattern` 属性：能解析，但**不生效**

### 0.5 最小可用样例（实测通过）

下面这段可直接存成 `app.vcf`，实测 `parse` 退出码 0、`validate` 退出码 0：

```vcf
# 应用配置（本样例已实测 parse + validate 通过）

#@schema {
  version = "1.0"
  app_name {
    type = "string"
    required = true
  }
  port {
    type = "integer"
    default = 8080
    range = 1024..65535
  }
}

app_name = "my-service"
port = 8080

server {
  host = "0.0.0.0"
  port = "${ENV:PORT or 8080}"
}
```

---

## Table of Contents

1. [Introduction](#introduction)
2. [File Structure](#file-structure)
3. [Basic Syntax](#basic-syntax)
4. [Data Types](#data-types)
5. [Advanced Features](#advanced-features)
6. [Schema System](#schema-system)
7. [Best Practices](#best-practices)
8. [Appendix](#appendix)

---

## Introduction

VerseConf (VCF) is a modern configuration language designed for the AI era. It combines human readability with powerful features like expressions, templates, and schema validation.

> **状态**：以上为**目标定位**。当前 `0.2.0` 实现覆盖的是其中一个子集——表达式与 schema 校验可用，模板不可用（见 [Advanced Features](#advanced-features)）。

### Design Principles

- **Human-First**: Clear syntax, extensive comments, helpful error messages
- **AI-Friendly**: Schema metadata helps LLMs generate accurate configurations
- **Expressive**: Support calculations, conditionals, and composition
- **Safe**: Built-in validation, security auditing, and type checking

> **状态**：⚠️ 部分实现。`#` 注释与 schema 元数据（`desc` / `example` / `llm_hint`）可解析；
> 计算（引号内表达式）可用；条件（`==`）与组合（模板继承）不可用。安全审计见 `verseconf audit` 子命令（本文未逐项实测，❓未验证）。

---

## File Structure

### File Extension

VerseConf files use the `.vcf` extension.

```
config.vcf
server.production.vcf
database.local.vcf
```

> **状态**：✅ 已实现。扩展名不参与解析，`parse` 接受任意文件名。

### Encoding

- UTF-8 encoding required
- Unix (LF) or Windows (CRLF) line endings supported
- BOM optional

> **状态**：⚠️ 部分实现，且**原描述有误**。
> - UTF-8：✅
> - LF：✅；CRLF：✅（实测 `key = "value"\r\nother = 42` 解析通过）
> - BOM：🚧 **不成立**。带 UTF-8 BOM 的文件被拒绝，报 `lexical error: unexpected character:`，
>   指向的正是文件首字符 U+FEFF。请把文件保存为**无 BOM** 的 UTF-8。

---

## Basic Syntax

### Comments

```vcf
# Single-line comment

###
Block comment
Spans multiple lines
###

key = "value"  # Inline comment
```

> **状态**：⚠️ 部分实现。
> - ✅ `#` 单行注释：`parse` 退出码 0
> - ✅ 行尾注释：`key = "value"  # Inline comment` 退出码 0
> - 🚧 `###` 块注释：退出码 1，
>   `parse error: expected '=' or ':', found bare_key(Block)`
>   （解析器把 `###` 之后的正文当成键值对）
>
> 需要多行说明时，请用连续的 `#` 行。

### Key-Value Pairs

```vcf
# Bare keys (alphanumeric, underscore, hyphen)
name = "VerseConf"
version = "1.5"          # 普通字符串示例，与语言版本无关
max_connections = 100

# Quoted keys (special characters)
"server.host" = "localhost"
'key-with-dashes' = "value"
```

> **状态**：⚠️ 部分实现。
> - ✅ 裸键（字母、数字、下划线、连字符）：退出码 0
> - 🚧 双引号键 ` "server.host" = "localhost" `：退出码 1，
>   `parse error: unexpected token in table block: string`
> - 🚧 单引号键 `'key-with-dashes'`：退出码 1，
>   `lexical error: unexpected character: '''`（单引号整体不被支持）
>
> 需要分层语义时，请改用**块表**（见 [Tables (Blocks)](#tables-blocks)）或 `host` 这类扁平键名。

### String Values

```vcf
# Basic string (double quotes)
description = "A configuration language"

# Multi-line string (triple quotes)
help_text = """
This is a multi-line string.
It preserves line breaks and indentation.
"""

# Raw string (no escape sequences)
path = r"C:\Users\name\file.txt"
```

> **状态**：⚠️ 部分实现。
> - ✅ 双引号字符串：退出码 0
> - 🚧 三引号字符串：退出码 1，`parse error: unexpected token in table block: string`
> - 🚧 原始字符串 `r"..."`：退出码 1，`parse error: expected value, found bare_key(r)`
>
> 多行文本目前只能写成多个普通字符串键，或用 `\n` 转义（转义行为未逐项实测，❓未验证）。

### Numbers

```vcf
# Integers
port = 8080
negative = -42

# Floats
ratio = 3.14159
scientific = 1.5e-10

# Hexadecimal
color = 0xFF5733

# Binary
flags = 0b1010_1111
```

> **状态**：⚠️ 部分实现（这是本节最需要注意的地方）。
> - ✅ `port = 8080`：退出码 0
> - 🚧 `negative = -42`：退出码 1，`parse error: expected value, found -`
> - ✅ `ratio = 3.14159`：退出码 0
> - 🚧 `scientific = 1.5e-10`：退出码 1；`1.5e10`（正指数）同样退出码 1
> - 🚧 `color = 0xFF5733`：退出码 1，`parse error: expected '=' or ':', found newline`
> - 🚧 `flags = 0b1010_1111`：退出码 1，同上
>
> 负数请写成字符串再在表达式里取负（表达式内的 `-` 运算符可用，见 [Expressions](#expressions)）。

### Booleans

```vcf
debug = true
production = false
```

> **状态**：✅ 已实现。退出码 0。

### Date and Time

```vcf
# ISO 8601 datetime
start_time = 2024-01-15T09:00:00Z

# Date only
release_date = 2024-06-01

# Time only
daily_backup = 02:30:00

# Duration
timeout = 30s
cache_ttl = 1h30m
retry_delay = 500ms
```

> **状态**：⚠️ 部分实现，逐条实测：
> - ✅ `2024-01-15T09:00:00Z`：退出码 0
> - ✅ `2024-06-01`：退出码 0
> - 🚧 `02:30:00`：退出码 1，`parse error: unexpected token in table block: =`
> - ✅ `timeout = 30s`：退出码 0；`1h`、`30m` 亦为退出码 0
> - 🚧 `cache_ttl = 1h30m`：退出码 1，`parse error: unexpected token in table block: duration(30m)`
> - 🚧 `retry_delay = 500ms`：退出码 1，`parse error: expected '=' or ':', found newline`
>
> 也就是说：**单位 `s` / `m` / `h` 可用，`ms` 不可用作裸值，复合时长不可用作裸值**。
> 毫秒仍可在**引号内表达式**里参与运算：`timeout = "${30s + 500ms}"` 实测退出码 0。

### Arrays

```vcf
# Simple array
ports = [8080, 8081, 8082]

# Mixed types (not recommended)
mixed = ["text", 42, true]

# Trailing comma allowed
items = [
  "first",
  "second",
  "third",
]
```

> **状态**：✅ 已实现。三种写法均为退出码 0。

### Tables (Blocks)

```vcf
# Block syntax
server {
  host = "localhost"
  port = 8080
  
  ssl {
    enabled = true
    cert = "/path/to/cert.pem"
  }
}

# Array of tables
database {
  name = "primary"
  host = "db1.example.com"
}

database {
  name = "replica"
  host = "db2.example.com"
}
```

> **状态**：✅ 已实现。嵌套块表、同名块重复出现（表数组）均为退出码 0。
> 内联表 `{ a = 1 }` 亦为退出码 0（见 [Type System Overview](#type-system-overview)）。

---

## Data Types

### Type System Overview

| Type | Description | Example | Status |
|------|-------------|---------|--------|
| `string` | UTF-8 text | `"hello"` | ✅ |
| `integer` | 64-bit signed | `42`, `-17` | ⚠️ `42` ✅；`-17` 🚧 |
| `float` | 64-bit IEEE 754 | `3.14`, `-0.5` | ⚠️ `3.14` ✅；`-0.5` 🚧；科学计数 🚧 |
| `bool` | Boolean | `true`, `false` | ✅ |
| `datetime` | ISO 8601 timestamp | `2024-01-01T00:00:00Z` | ✅ |
| `duration` | Time span | `1h30m`, `45s` | ⚠️ `45s` / `1h` / `30m` ✅；`1h30m` 🚧；`500ms` 🚧 |
| `array` | Ordered list | `[1, 2, 3]` | ✅ |
| `table` | Key-value map | `{ a = 1 }` | ✅ 内联表与块表均可用 |

### Type Coercion

```vcf
# Automatic coercion in expressions
port = "8080"      # String
port_num = ${port} # Coerced to integer in expression
```

> **状态**：🚧 未实现（写法层面）。第二行是**裸表达式**，实测退出码 1：
> `lexical error: unexpected character: '$'`。
>
> 实测可用的等价写法（退出码 0）：
>
> ```vcf
> port = "8080"        # String
> port_num = "${port}"  # 引号内表达式，变量引用可用
> ```
>
> 注意：这里只验证了**解析通过**；字符串到整数的隐式转换语义是否发生，❓未验证。

---

## Advanced Features

### Expressions

Expressions allow dynamic value calculation.

```vcf
# Arithmetic
http_port = 8080
https_port = ${http_port + 443}

# Duration arithmetic
timeout = ${30s + 500ms}
cache_ttl = ${1h + 30m}

# String concatenation
prefix = "api"
path = ${"/" + prefix + "/v1"}

# Environment variables
host = ${ENV:HOSTNAME}
database_url = ${ENV:DATABASE_URL}

# Default values
port = ${ENV:PORT} or 8080
```

> **状态**：🚧 未实现（**整段照抄会全部报错**）。核心规则只有一条：
> **`${...}` 必须写在双引号内**；裸 `${...}` 一律 `lexical error: unexpected character: '$'`。
>
> 逐条实测：
>
> | 目标写法 | 结果 |
> |----------|------|
> | `https_port = ${http_port + 443}` | 🚧 退出码 1（lexical error: `'$'`） |
> | `https_port = "${http_port + 443}"` | ✅ 退出码 0 |
> | `timeout = ${30s + 500ms}` | 🚧 退出码 1 |
> | `timeout = "${30s + 500ms}"` | ✅ 退出码 0 |
> | `cache_ttl = "${1h + 30m}"` | ✅ 退出码 0 |
> | `path = ${"/" + prefix + "/v1"}` | 🚧 退出码 1 |
> | `path = "${"/" + prefix + "/v1"}"` | 🚧 退出码 1，`parse error: expected expression value, found bare_key(v1)`（**表达式内不能再嵌引号**） |
> | `host = ${ENV:HOSTNAME}` | 🚧 退出码 1 |
> | `host = "${ENV:HOSTNAME}"` | ✅ 退出码 0 |
> | `port = ${ENV:PORT} or 8080` | 🚧 退出码 1 |
> | `port = "${ENV:PORT or 8080}"` | ✅ 退出码 0 |
> | `timeout = 30s + 15s`（无 `${}`） | ✅ 退出码 0 |
>
> 另有实测发现、原文未提及的一条：`${environment == "production"}` 🚧 退出码 1（表达式内嵌引号与 `==` 运算符均不可用）。

#### Expression Operators

| Operator | Description | Example | Status |
|----------|-------------|---------|--------|
| `+` | Addition | `${a + b}` | ✅（须在引号内） |
| `-` | Subtraction | `${a - b}` | ✅（须在引号内） |
| `*` | Multiplication | `${a * b}` | ✅（须在引号内） |
| `/` | Division | `${a / b}` | ✅（须在引号内） |
| `or` | Default value | `${a or b}` | ✅（须在引号内） |
| `==` | Equality | `${a == "x"}` | 🚧 未实现（退出码 1） |

> **状态**：⚠️ 部分实现。运算符本身的可用性以"引号内表达式"为前提；
> 上表 `+ - * / or` 五项的实测样例见本节表格，均为退出码 0。

### File Inclusion

Include other configuration files with merge strategies.

```vcf
# Simple include
@include "base.vcf"

# Include with merge strategy
@include "database.vcf" merge=deep_merge
@include "overrides.vcf" merge=shallow_merge

# Conditional include
@include "local.vcf" if=${ENV:LOCAL_DEV}
```

> **状态**：⚠️ 部分实现。
> - ✅ `@include "base.vcf"`：指令语法可用（用 `parse --no-include` 实测退出码 0；
>   默认模式下会去读被包含文件，文件不存在时报
>   `include error: Failed to read file ...`）
> - ✅ `@include "database.vcf" merge=deep_merge`：语法可用（`--no-include`，退出码 0）
> - ✅ `@include "local.vcf" if="true"`：语法可用（`--no-include`，退出码 0）
> - 🚧 `@include "local.vcf" if=${ENV:LOCAL_DEV}`：退出码 1，
>   `lexical error: unexpected character: '$'`
>
> **注意**：默认情况下 `parse` 会解析并读取 `@include` 指向的文件；
> 只检查语法请加 `--no-include`。

#### Merge Strategies

| Strategy | Description | Status |
|----------|-------------|--------|
| `shallow_merge` | Top-level keys only | ✅ 语法接受；合并语义 ❓未验证 |
| `deep_merge` | Recursive merge of nested tables | ✅ 语法接受；合并语义 ❓未验证 |
| `replace` | Complete replacement | ❓未验证（本文未逐条实测该取值） |

### Templates

Templates enable configuration inheritance.

```vcf
#@template base {
  server {
    host = "0.0.0.0"
    timeout = 30s
  }
}

#@template production : base {
  server {
    workers = 8
    ssl {
      enabled = true
    }
  }
}

# Use template
#@use production

app_name = "my-app"
```

> **状态**：🚧 未实现（模板继承整体不可用）。
> - 🚧 `#@template base {`：退出码 1，`parse error: expected metadata key, found {`
> - 🚧 `#@template production : base {`：退出码 1，同上
> - ✅ `#@use production`：**单独出现时**退出码 0（被当作普通元数据接受），
>   但由于模板定义本身不可用，它没有任何可继承的对象——**语义未实现**。
>
> 需要复用时，请使用 `@include` + 覆盖（见 [File Inclusion](#file-inclusion)）。

### Metadata

Metadata provides additional context for tools and AI.

```vcf
#@deprecated "Use new_config instead"
old_key = "value"

#@experimental
new_feature = true

#@sensitive
api_key = "secret123"

#@llm_hint "Production should use 443 or 8443"
port = 8080
```

> **状态**：⚠️ 部分实现。**带字符串参数的元数据不可用**：
>
> | 写法 | 结果 |
> |------|------|
> | `#@experimental` | ✅ 退出码 0 |
> | `#@sensitive` | ✅ 退出码 0 |
> | `#@deprecated "Use new_config instead"` | 🚧 退出码 1，`parse error: expected metadata key, found string(...)` |
> | `#@llm_hint "Production should use 443 or 8443"` | 🚧 退出码 1，同上 |
>
> 另有一条实测发现的组合限制：`#@strict true` 与 `#@schema { ... }`
> **写在同一个文件里会解析失败**（无论先后，退出码 1，`parse error: expected metadata key, found {` / `found bool(true)`）。
> 两者请分文件使用，或改用 `validate --strict` 命令行开关。

---

## Schema System

Schema provides type validation and AI guidance.

### Schema Definition

```vcf
#@schema {
  version = "1.0"
  description = "Web server configuration"
  
  app_name {
    type = "string"
    required = true
    desc = "Application identifier"
    example = "my-service"
    llm_hint = "Use lowercase with hyphens"
  }
  
  port {
    type = "integer"
    default = 8080
    range = 1024..65535
    desc = "HTTP listen port"
    llm_hint = "Production: use 443 or 8443"
  }
  
  debug {
    type = "bool"
    default = false
    desc = "Enable debug mode"
    llm_hint = "Never enable in production"
  }
  
  database {
    type = "table"
    required = true
    
    host {
      type = "string"
      default = "localhost"
      desc = "Database host"
    }
    
    port {
      type = "integer"
      default = 5432
      range = 1..65535
    }
    
    password {
      type = "string"
      required = true
      sensitive = true
      desc = "Database password"
      llm_hint = "Use environment variable: ${ENV:DB_PASSWORD}"
    }
  }
  
  log_level {
    type = "string"
    default = "info"
    enum = ["debug", "info", "warn", "error"]
    desc = "Logging verbosity"
  }
}
```

> **状态**：✅ 已实现（解析层面）。上面这段结构（含 `range = 1024..65535`、`enum`、
> 嵌套字段、`required`、`sensitive`、`deprecated`、`llm_hint`、`example` 属性）
> 实测 `parse` 退出码 0。
>
> 其中 `llm_hint` 里出现的 `${ENV:DB_PASSWORD}` 是**说明文本**，位于字符串内部，不参与表达式解析。
>
> 校验行为见 [Schema Field Attributes](#schema-field-attributes)。

### Schema Field Attributes

| Attribute | Type | Description | Status |
|-----------|------|-------------|--------|
| `type` | string | Data type | ✅ 解析通过 |
| `required` | bool | Must be provided | ✅ 生效：`validate` 报 `missing required field` |
| `default` | any | Default value if not specified | ❓未验证（默认值填充行为未实测） |
| `range` | expression | Valid range for numbers (e.g., `1..100`) | ✅ 生效：`validate` 报 `value 70000 for field 'port' exceeds maximum 65535` |
| `enum` | array | Allowed values | ✅ 生效：`validate` 报 `is not in allowed values` |
| `pattern` | string | Regex pattern for strings | 🚧 **不生效**：`code = "ABC"` 未通过 `^[a-z]+$` 仍报 `Configuration is valid!` |
| `desc` | string | Human-readable description | ❓未验证（元数据，未实测其消费方） |
| `example` | string | Example value | ❓未验证 |
| `llm_hint` | string | Guidance for AI generation | ❓未验证 |
| `sensitive` | bool | Mark as sensitive data | ❓未验证（`verseconf audit` 是否消费该标记未实测） |

> **实测命令**：`verseconf validate <file>`。上表的 ✅/🚧 均指 `validate` 的实际行为。

### Strict Mode

Enable strict validation to catch undefined fields.

```vcf
#@strict true

# Only schema-defined fields allowed
app_name = "my-app"  # ✓ Valid
unknown_key = "value"  # ✗ Error in strict mode
```

> **状态**：⚠️ 部分实现，且**开关位置与原文不同**。
> - ✅ `#@strict true` 单独出现时可解析（退出码 0）
> - 🚧 但它**不会**让 `validate`（不带参数）拒绝未声明字段：实测 `Configuration is valid!`
> - ✅ 真正的严格模式开关是命令行参数：`verseconf validate --strict <file>`，
>   实测报 `validation error: undeclared field 'unknown' in strict mode`
> - ⚠️ 且 `--strict` **需要文件里有 `#@schema` 块**才有效；没有 schema 时，
>   同一文件加 `--strict` 仍报 `Configuration is valid!`
>
> 结论：请用 `verseconf validate --strict`，不要依赖文件内的 `#@strict true`。

---

## Best Practices

> 本节是**写作建议**，不是语法。下面的示例已按当前实现修正；原文中会报错的写法在旁注中给出。

### Organization

```
project/
├── config/
│   ├── base.vcf           # Shared defaults
│   ├── database.vcf       # Database config
│   ├── cache.vcf          # Cache config
│   └── schema.vcf         # Schema definitions
├── environments/
│   ├── development.vcf    # Dev overrides
│   ├── staging.vcf        # Staging overrides
│   └── production.vcf     # Production overrides
└── local.vcf.example      # Template for local config
```

> **状态**：✅ 目录结构建议与实现无关。跨文件复用请用 `@include`（[File Inclusion](#file-inclusion)）。

### Naming Conventions

```vcf
# Use lowercase with underscores for keys
database_host = "localhost"      # ✓ Good
databaseHost = "localhost"       # ✗ Avoid camelCase
DatabaseHost = "localhost"       # ✗ Avoid PascalCase

# Use descriptive names
connection_timeout = 30s         # ✓ Good
timeout = 30s                    # ✗ Too vague
t = 30                           # ✗ Too short

# Group related configs
database {
  host = "localhost"
  port = 5432
  name = "myapp"
}
```

> **状态**：✅ 已实现（本段整段实测退出码 0；驼峰/帕斯卡键只是"不推荐"，语法上仍可解析）。

### Security

```vcf
# Never commit secrets
#@sensitive
api_key = ${ENV:API_KEY}         # ✓ Good
api_key = "hardcoded-secret"     # ✗ Never do this

# Use environment variables for sensitive data
database {
  password = ${ENV:DB_PASSWORD}  # ✓ Good
}
```

> **状态**：🚧 未实现（写法层面）。原文两处 `${ENV:...}` 都是**裸表达式**，退出码 1。
>
> 实测可用的修正版（退出码 0）：
>
> ```vcf
> # Never commit secrets
> #@sensitive
> api_key = "${ENV:API_KEY}"         # ✓ 引号内表达式
>
> database {
>   password = "${ENV:DB_PASSWORD}"  # ✓ 引号内表达式
> }
> ```
>
> `#@sensitive` 标记本身可解析；它是否被 `verseconf audit` 消费，❓未验证。

### Comments and Documentation

```vcf
###
Application Configuration
=========================

This file defines the main application settings.
For environment-specific overrides, see environments/ directory.
###

# Server Configuration
server {
  # Host to bind to
  # Use 0.0.0.0 for all interfaces, 127.0.0.1 for localhost only
  host = "0.0.0.0"
  
  # Port number
  # Must be above 1024 for non-root users
  port = 8080
}
```

> **状态**：🚧 未实现（文件头的 `###` 块注释）。实测退出码 1。
>
> 实测可用的修正版（退出码 0）：把 `###` 换成逐行 `#`：
>
> ```vcf
> # Application Configuration
> # =========================
> #
> # This file defines the main application settings.
> # For environment-specific overrides, see environments/ directory.
>
> # Server Configuration
> server {
>   # Host to bind to
>   # Use 0.0.0.0 for all interfaces, 127.0.0.1 for localhost only
>   host = "0.0.0.0"
>
>   # Port number
>   # Must be above 1024 for non-root users
>   port = 8080
> }
> ```

### Version Control

```vcf
#@schema {
  version = "1.0"  # Schema version for migrations
}

# Track config version
config_version = "1.2.3"
```

> **状态**：✅ 已实现。整段实测退出码 0。

---

## Appendix

### A. Complete Example

#### A.1 目标写法（照抄会报错）

下面是本文档原本给出的完整示例。它包含多处 🚧 写法（`###` 块注释、裸表达式、
`@include` 指向的文件、`==` 运算符），**直接复制到文件里 `parse` 会失败**：

```vcf
###
Production Web Server Configuration
====================================

Schema version: 1.0
Config version: 2.1.0
Last updated: 2024-01-15
###

#@schema {
  version = "1.0"
  description = "Production web server configuration"
  
  app_name {
    type = "string"
    required = true
    desc = "Application name"
    example = "api-gateway"
  }
  
  environment {
    type = "string"
    enum = ["development", "staging", "production"]
    default = "development"
  }
}

# Application
app_name = "my-service"
environment = "production"
version = "2.1.0"

# Server Configuration
server {
  host = "0.0.0.0"
  port = ${ENV:PORT} or 8080
  workers = ${cpu_cores * 2}
  
  ssl {
    enabled = true
    port = 8443
    cert = "/etc/ssl/certs/server.crt"
    key = "/etc/ssl/private/server.key"
  }
  
  limits {
    max_body_size = "10MB"
    timeout = 30s
    keep_alive = 75s
  }
}

# Database
@include "database.production.vcf" merge=deep_merge

# Cache
redis {
  host = "redis.internal"
  port = 6379
  ttl = ${1h}
}

# Logging
logging {
  level = "info"
  format = "json"
  output = "stdout"
  
  filters = [
    "actix_web=warn",
    "my_app=debug",
  ]
}

# Feature Flags
features {
  new_dashboard = true
  beta_api = false
  analytics = ${environment == "production"}
}
```

> **状态**：🚧 未实现。其中 🚧 条目：`###` 块注释、`port = ${ENV:PORT} or 8080`、
> `workers = ${cpu_cores * 2}`、`ttl = ${1h}`、`analytics = ${environment == "production"}`。
> `@include` 一行需要 `database.production.vcf` 实际存在，否则报 `include error`。

#### A.2 实测可解析的完整示例

下面这版逐条改成了当前实现支持的写法，**实测 `parse` 与 `validate` 均退出码 0**：

```vcf
# Production Web Server Configuration
# Schema version: 1.0
# Config version: 2.1.0
# Last updated: 2024-01-15

#@schema {
  version = "1.0"
  description = "Production web server configuration"

  app_name {
    type = "string"
    required = true
    desc = "Application name"
    example = "api-gateway"
  }

  environment {
    type = "string"
    enum = ["development", "staging", "production"]
    default = "development"
  }
}

app_name = "my-service"
environment = "production"
version = "2.1.0"

cpu_cores = 4

server {
  host = "0.0.0.0"
  port = "${ENV:PORT or 8080}"
  workers = "${cpu_cores * 2}"

  ssl {
    enabled = true
    port = 8443
    cert = "/etc/ssl/certs/server.crt"
    key = "/etc/ssl/private/server.key"
  }

  limits {
    max_body_size = "10MB"
    timeout = 30s
    keep_alive = 75s
  }
}

redis {
  host = "redis.internal"
  port = 6379
  ttl = "${1h}"
}

logging {
  level = "info"
  format = "json"
  output = "stdout"

  filters = [
    "actix_web=warn",
    "my_app=debug",
  ]
}

features {
  new_dashboard = true
  beta_api = false
  analytics = true   # 目标写法 "${environment == "production"}" 未实现，见 A.1
}
```

> 与原示例的差异：`###` 头注释 → `#` 行；裸 `${...}` → 引号内 `${...}`；
> `workers` 依赖的 `cpu_cores` 显式声明；`analytics` 用字面量替代不可用的 `==` 表达式；
> 去掉了需要外部文件的 `@include`（如需保留，请加 `parse --no-include` 或确保被包含文件存在）。

### B. Error Messages

VerseConf provides helpful error messages:

```
Error: Invalid value for field 'port'
  --> config.vcf:15:9
   |
15 | port = 70000
   |         ^^^^^
   |
   = Expected: integer in range 1024..65535
   = Schema: port { type = "integer", range = 1024..65535 }
   = Hint: Common ports: 8080 (HTTP), 8443 (HTTPS), 3000 (dev)
```

> **状态**：⚠️ 上例是**示意格式，不是逐字输出**。当前实现的真实输出（实测 `verseconf validate`）形如：
>
> ```
> config.vcf:2:3: validation error: value 70000 for field 'port' exceeds maximum 65535
>   --> config.vcf:2:3
> ```
>
> 实测确认可用的错误类别：`lexical error`、`parse error`、`validation error`、`include error`。
> 行列号指向 schema 字段声明处，而非出错的值所在行。

### C. Migration Guide

When upgrading schema versions:

1. Update schema version field
2. Add new fields with defaults
3. Mark deprecated fields
4. Test with validation

```vcf
#@schema {
  version = "1.1"  # Upgraded from 1.0
  
  # New field
  new_option {
    type = "string"
    default = "default_value"  # Safe default
  }
  
  # Deprecated field
  old_option {
    type = "string"
    deprecated = "Use new_option instead"
  }
}
```

> **状态**：✅ 已实现。整段实测退出码 0（`deprecated` 作为 **schema 字段属性**可用；
> 注意它与 [Metadata](#metadata) 里不可用的 `#@deprecated "msg"` 指令是两回事）。

---

## 附录 D. 实测记录

### D.1 复现方式

所有状态判定都来自仓库内已编译的 CLI（`repo/target/release/verseconf.exe`）：

```bash
verseconf parse    <file>            # 退出码 0 = 接受；1 = 拒绝（错误打到 stderr）
verseconf parse    --no-include <f>  # 只检查语法，不读取 @include 指向的文件
verseconf validate <file>            # 附加 schema 校验
verseconf validate --strict <file>   # 拒绝未声明字段（需文件内有 #@schema）
verseconf --version                  # 实测输出 verseconf 0.2.0
```

### D.2 判定摘要

- 共 **94 个单点探针文件**，每个文件只放一条语法点（避免互相干扰），各执行一次 `parse`：
  **54 次退出码 0（✅）**，**40 次退出码 1（🚧）**。
- 另有 **12 次 `validate` 实测**，用于判定 `range` / `required` / `enum` / `pattern` / `--strict` 的语义。
- 每条 🚧 的具体错误信息已写在对应章节的"状态"块中。
- 说明：`@include` 相关的三条判定用 `parse --no-include` 单独复核——默认模式下会因被包含文件不存在而报
  `include error`，无法区分「语法不支持」与「文件缺失」。

### D.3 需要特别提醒的三条

1. **`${...}` 必须写在引号内**——这是与原文差异最大、最容易踩的一条。
2. **BOM 不被支持**——原文写"BOM optional"，实测被拒绝。
3. **`#@strict true` 与 `#@schema` 不能同文件共存**——请改用 `validate --strict`。

---

**Specification Version**: 1.5（目标规范；实现版本 0.2.0）  
**Last Updated**: 2026-09-23（补充状态标注与实测依据）  
**Maintainer**: VerseConf Team
