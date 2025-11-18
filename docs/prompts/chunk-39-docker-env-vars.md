# Chunk 39: Docker Environment Variables

## Context
Frontend is complete. Now update Docker configuration for new environment variables.

## Goal
Update compose.yml and .env.example with all Phase 1 configuration options.

## Prompt

```text
I have the frontend complete. Now update Docker configuration.

Create/update .env.example:

```bash
# RUS - Rust URL Shortener Configuration
# Copy this file to .env and customize values

# REQUIRED: JWT Secret Key
# Generate with: openssl rand -base64 32
JWT_SECRET=your-secret-key-here-change-this

# Application URLs
HOST_URL=http://localhost:8080

# Server Configuration
HOST=0.0.0.0
PORT=8080

# Database
DB_PATH=./data/rus.db

# Authentication
JWT_EXPIRY=1                        # Hours until access token expires (default: 1)
REFRESH_TOKEN_EXPIRY=7              # Days until refresh token expires (default: 7)

# Security
ACCOUNT_LOCKOUT_ATTEMPTS=5          # Failed attempts before lockout (default: 5)
ACCOUNT_LOCKOUT_DURATION=30         # Lockout duration in minutes (default: 30)

# URL Validation
MAX_URL_LENGTH=2048                 # Maximum URL length (default: 2048)

# Analytics
CLICK_RETENTION_DAYS=30             # Days to keep click history (default: 30)
```

Update compose.yml:

```yaml
services:
  app:
    build:
      context: .
      dockerfile: Dockerfile
    ports:
      - "${PORT:-8080}:${PORT:-8080}"
    volumes:
      - ./data:/app/data
    environment:
      - HOST=${HOST:-0.0.0.0}
      - PORT=${PORT:-8080}
      - RUST_LOG=info
    env_file:
      - .env
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:${PORT:-8080}/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s
    networks:
      - rus-network
    deploy:
      resources:
        limits:
          memory: 512M

networks:
  rus-network:
    driver: bridge
```

Key additions:
1. All environment variables documented in .env.example
2. PORT can be customized (not hardcoded 8080)
3. Health check uses /health endpoint
4. Memory limit set (512M is plenty for Phase 1)
5. Start period gives app time to initialize
6. env_file loads all variables

The health check:
- Uses curl to hit /health endpoint
- Checks every 30 seconds
- Marks unhealthy after 3 failed attempts
- Allows 10 second timeout per check
- Gives 10 second start period before checking

Resource limits:
- 512MB memory should be more than enough
- Prevents runaway memory usage

Make sure the Dockerfile installs curl for health checks:

```dockerfile
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
```
```

## Expected Output
- .env.example with all variables documented
- compose.yml with flexible port
- Health check configuration
- Memory limits
- Proper env_file loading
- curl installed for health checks
