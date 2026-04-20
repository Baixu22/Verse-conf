# VerseConf Tutorial

Learn VerseConf from basics to advanced features in 15 minutes.

## Table of Contents

1. [Installation](#installation)
2. [Your First Config](#your-first-config)
3. [Basic Syntax](#basic-syntax)
4. [Working with Tables](#working-with-tables)
5. [Using Expressions](#using-expressions)
6. [File Organization](#file-organization)
7. [Adding Schema](#adding-schema)
8. [CLI Tools](#cli-tools)

---

## Installation

```bash
# Install from crates.io
cargo install verseconf-cli

# Verify installation
verseconf --version
```

---

## Your First Config

Create a file named `app.vcf`:

```vcf
# My Application Configuration
app_name = "hello-world"
version = "1.0.0"
port = 8080
debug = true
```

Parse it:

```bash
verseconf parse app.vcf
```

Output:
```json
{
  "app_name": "hello-world",
  "version": "1.0.0",
  "port": 8080,
  "debug": true
}
```

---

## Basic Syntax

### Comments

```vcf
# This is a single-line comment

###
This is a block comment.
It can span multiple lines.
###

key = "value"  # Comments can be inline too
```

### Strings

```vcf
# Double quotes (escape sequences supported)
name = "Hello \"World\""

# Multi-line strings
description = """
This is a multi-line string.
Line breaks are preserved.
"""

# Raw strings (no escaping)
path = r"C:\Users\name\file.txt"
```

### Numbers

```vcf
# Integers
port = 8080
count = -42

# Floats
ratio = 3.14159

# Hexadecimal
color = 0xFF5733
```

### Booleans

```vcf
enabled = true
disabled = false
```

### Arrays

```vcf
# Simple array
ports = [8080, 8081, 8082]

# Mixed types
settings = ["debug", 42, true]

# Trailing comma is allowed
items = [
  "first",
  "second",
]
```

---

## Working with Tables

Tables group related configuration.

### Basic Table

```vcf
server {
  host = "localhost"
  port = 8080
}
```

### Nested Tables

```vcf
database {
  host = "localhost"
  port = 5432
  
  credentials {
    username = "admin"
    password = "secret"
  }
}
```

### Array of Tables

```vcf
# Multiple databases
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

## Using Expressions

Expressions make your configs dynamic.

### Arithmetic

```vcf
base_port = 8000
http_port = ${base_port}
https_port = ${base_port + 443}
```

### Environment Variables

```vcf
# Use environment variables
host = ${ENV:HOSTNAME}
port = ${ENV:PORT}

# With default values
port = ${ENV:PORT} or 8080
```

### Duration Calculations

```vcf
# Duration arithmetic
timeout = ${30s + 500ms}
cache_ttl = ${1h + 30m}
```

### String Concatenation

```vcf
prefix = "api"
version = "v1"
base_path = ${"/" + prefix + "/" + version}
# Result: "/api/v1"
```

---

## File Organization

### Include Other Files

```vcf
# Include base configuration
@include "base.vcf"

# Include with merge strategy
@include "database.vcf" merge=deep_merge

# Conditional include
@include "local.vcf" if=${ENV:LOCAL_DEV}
```

### Project Structure Example

```
my-project/
├── config/
│   ├── base.vcf           # Shared defaults
│   ├── database.vcf       # Database settings
│   └── cache.vcf          # Cache settings
├── environments/
│   ├── development.vcf    # Dev overrides
│   ├── staging.vcf        # Staging overrides
│   └── production.vcf     # Production overrides
└── app.vcf                # Main config
```

### Main Config

```vcf
# app.vcf
@include "config/base.vcf"
@include "config/database.vcf" merge=deep_merge
@include "config/cache.vcf" merge=deep_merge

# Environment-specific overrides
@include "environments/${ENV:ENVIRONMENT}.vcf" if=${ENV:ENVIRONMENT}

# Application-specific settings
app_name = "my-app"
version = "1.0.0"
```

---

## Adding Schema

Schema validates your configuration and helps AI tools.

### Basic Schema

```vcf
#@schema {
  app_name {
    type = "string"
    required = true
    desc = "Application name"
  }
  
  port {
    type = "integer"
    default = 8080
    range = 1024..65535
    desc = "Server port"
  }
  
  debug {
    type = "bool"
    default = false
  }
}

# Your config
app_name = "my-app"
port = 3000
```

### Schema with AI Hints

```vcf
#@schema {
  database {
    type = "table"
    required = true
    
    host {
      type = "string"
      default = "localhost"
      desc = "Database host"
      llm_hint = "Use environment variable for production"
    }
    
    password {
      type = "string"
      required = true
      sensitive = true
      desc = "Database password"
      llm_hint = "Never hardcode. Use: ${ENV:DB_PASSWORD}"
    }
  }
}

database {
  host = "localhost"
  password = ${ENV:DB_PASSWORD}
}
```

### Validate Your Config

```bash
# Basic validation
verseconf validate app.vcf

# Strict mode (catches undefined fields)
verseconf validate app.vcf --strict

# Auto-fix issues
verseconf validate app.vcf --fix --dry-run  # Preview changes
verseconf validate app.vcf --fix --write    # Apply changes
```

---

## CLI Tools

### Parse and Format

```bash
# Parse config to JSON
verseconf parse app.vcf

# Format config
verseconf format app.vcf

# AI-friendly formatting
verseconf format app.vcf --ai-canonical
```

### Generate Documentation

```bash
# Generate Markdown docs from schema
verseconf doc app.vcf > CONFIG.md
```

### Compare Configs

```bash
# Show differences
verseconf diff app.vcf app.new.vcf

# Compare environments
verseconf diff environments/development.vcf environments/production.vcf
```

### Environment Commands

```bash
# Show effective config for an environment
verseconf env --environment production

# Validate all environments
verseconf env --validate-all
```

### Security Audit

```bash
# Check for security issues
verseconf audit app.vcf

# Example output:
# ⚠️  Found 2 potential issues:
#    - Hardcoded password at line 15
#    - Missing SSL configuration
```

---

## Complete Example

Here's a production-ready configuration:

```vcf
###
My Application - Production Configuration
==========================================

This is the main configuration file for the production environment.
For local development, see environments/development.vcf
###

#@schema {
  version = "1.0"
  description = "Production web application configuration"
  
  app_name {
    type = "string"
    required = true
    desc = "Application identifier"
    example = "api-gateway"
  }
  
  environment {
    type = "string"
    enum = ["development", "staging", "production"]
    default = "development"
  }
  
  server {
    type = "table"
    
    port {
      type = "integer"
      default = 8080
      range = 1024..65535
      desc = "HTTP server port"
      llm_hint = "Production should use 443 or 8443"
    }
    
    workers {
      type = "integer"
      default = 4
      desc = "Number of worker processes"
      llm_hint = "Recommended: number of CPU cores * 2"
    }
  }
}

# Application Settings
app_name = "my-awesome-app"
environment = "production"
version = "2.0.0"

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
  }
}

# Include database config
@include "database.production.vcf" merge=deep_merge

# Include cache config
@include "cache.production.vcf" merge=deep_merge

# Feature Flags
features {
  new_ui = true
  beta_features = false
  analytics = true
}
```

---

## Next Steps

- Read the [full specification](SPECIFICATION.md)
- Explore [example configurations](../examples/)
- Check out the [Rust API documentation](https://docs.rs/verseconf-core)
- Join our community discussions

---

**Happy Configuring!** 🚀
