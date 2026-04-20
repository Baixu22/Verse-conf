# VerseConf Language Specification v1.5

## Table of Contents

1. [Introduction](#introduction)
2. [File Structure](#file-structure)
3. [Basic Syntax](#basic-syntax)
4. [Data Types](#data-types)
5. [Advanced Features](#advanced-features)
6. [Schema System](#schema-system)
7. [Best Practices](#best-practices)

---

## Introduction

VerseConf (VCF) is a modern configuration language designed for the AI era. It combines human readability with powerful features like expressions, templates, and schema validation.

### Design Principles

- **Human-First**: Clear syntax, extensive comments, helpful error messages
- **AI-Friendly**: Schema metadata helps LLMs generate accurate configurations
- **Expressive**: Support calculations, conditionals, and composition
- **Safe**: Built-in validation, security auditing, and type checking

---

## File Structure

### File Extension

VerseConf files use the `.vcf` extension.

```
config.vcf
server.production.vcf
database.local.vcf
```

### Encoding

- UTF-8 encoding required
- Unix (LF) or Windows (CRLF) line endings supported
- BOM optional

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

### Key-Value Pairs

```vcf
# Bare keys (alphanumeric, underscore, hyphen)
name = "VerseConf"
version = "1.5"
max_connections = 100

# Quoted keys (special characters)
"server.host" = "localhost"
'key-with-dashes' = "value"
```

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

### Booleans

```vcf
debug = true
production = false
```

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

---

## Data Types

### Type System Overview

| Type | Description | Example |
|------|-------------|---------|
| `string` | UTF-8 text | `"hello"` |
| `integer` | 64-bit signed | `42`, `-17` |
| `float` | 64-bit IEEE 754 | `3.14`, `-0.5` |
| `bool` | Boolean | `true`, `false` |
| `datetime` | ISO 8601 timestamp | `2024-01-01T00:00:00Z` |
| `duration` | Time span | `1h30m`, `45s` |
| `array` | Ordered list | `[1, 2, 3]` |
| `table` | Key-value map | `{ a = 1 }` |

### Type Coercion

```vcf
# Automatic coercion in expressions
port = "8080"      # String
port_num = ${port} # Coerced to integer in expression
```

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

#### Expression Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `+` | Addition | `${a + b}` |
| `-` | Subtraction | `${a - b}` |
| `*` | Multiplication | `${a * b}` |
| `/` | Division | `${a / b}` |
| `or` | Default value | `${a or b}` |

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

#### Merge Strategies

| Strategy | Description |
|----------|-------------|
| `shallow_merge` | Top-level keys only |
| `deep_merge` | Recursive merge of nested tables |
| `replace` | Complete replacement |

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

### Schema Field Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `type` | string | Data type (string, integer, float, bool, datetime, duration, array, table) |
| `required` | bool | Must be provided |
| `default` | any | Default value if not specified |
| `range` | expression | Valid range for numbers (e.g., `1..100`) |
| `enum` | array | Allowed values |
| `pattern` | string | Regex pattern for strings |
| `desc` | string | Human-readable description |
| `example` | string | Example value |
| `llm_hint` | string | Guidance for AI generation |
| `sensitive` | bool | Mark as sensitive data |

### Strict Mode

Enable strict validation to catch undefined fields.

```vcf
#@strict true

# Only schema-defined fields allowed
app_name = "my-app"  # ✓ Valid
unknown_key = "value"  # ✗ Error in strict mode
```

---

## Best Practices

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

### Version Control

```vcf
#@schema {
  version = "1.0"  # Schema version for migrations
}

# Track config version
config_version = "1.2.3"
```

---

## Appendix

### A. Complete Example

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

---

**Specification Version**: 1.5  
**Last Updated**: 2024-01-20  
**Maintainer**: VerseConf Team
