# VerseConf 服务器配置模板
# 使用 {{variable}} 语法定义变量

#@ version=1.0, template="server"

server {
    host = "{{HOST}}"
    port = {{PORT}}
    workers = {{WORKERS}}
    
    ssl {
        enabled = {{SSL_ENABLED}}
        cert_path = "{{SSL_CERT_PATH}}"
        key_path = "{{SSL_KEY_PATH}}"
    }
    
    logging {
        level = "{{LOG_LEVEL}}"
        output = "{{LOG_OUTPUT}}"
    }
}

database {
    host = "{{DB_HOST}}"
    port = {{DB_PORT}}
    name = "{{DB_NAME}}"
    username = "{{DB_USER}}"
    password = "{{DB_PASSWORD}}" #@ sensitive
    
    pool {
        min_connections = {{DB_POOL_MIN}}
        max_connections = {{DB_POOL_MAX}}
        idle_timeout = {{DB_IDLE_TIMEOUT}}s
    }
}

cache {
    enabled = {{CACHE_ENABLED}}
    backend = "{{CACHE_BACKEND}}"
    
    redis {
        host = "{{REDIS_HOST}}"
        port = {{REDIS_PORT}}
        db = {{REDIS_DB}}
    }
    
    default_ttl = {{CACHE_TTL}}s
}
