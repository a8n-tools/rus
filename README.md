# rus (0.4.0)

**Rust URL Shortener** - A fast, secure URL shortening service built with Rust and Actix-web. Supports two deployment modes: standalone with built-in auth, or SaaS for integration with a parent application.

![URL Shortener Homepage](/assets/screenshot.png)

## Features

- **JWT Authentication** - Secure user registration and login with Argon2id password hashing
- **SQLite Persistence** - Reliable data storage with SQLite (bundled, zero setup)
- **Click Tracking** - Per-click history with configurable retention and analytics
- **QR Code Generation** - Generate QR codes for shortened URLs (PNG and SVG)
- **Custom Names** - Give your shortened URLs memorable names
- **URL Management** - Create, rename, delete, and monitor URLs
- **Admin Panel** - User management and abuse report review
- **Abuse Reporting** - Public abuse reporting for malicious URLs
- **Account Security** - Login attempt tracking with configurable lockout
- **Rate Limiting** - Built-in request rate limiting via actix-governor
- **Refresh Tokens** - Seamless token refresh without re-login
- **Dual Build Modes** - Standalone or SaaS deployment
- **Docker Support** - Multi-stage Dockerfile with dependency caching

## Build Modes

### Standalone (default)

Full-featured URL shortener with built-in user management:
- User registration and login with JWT authentication
- Password hashing with Argon2id (with automatic bcrypt migration)
- Admin user management
- Account lockout protection
- Refresh token rotation

```bash
cargo build --release --features standalone
```

### SaaS

Lightweight version for integration with a parent application:
- No built-in user management (uses external auth via `access_token` cookie)
- User identity extracted from parent app's JWT
- No registration/login routes
- Dashboard redirects to parent app if no valid session

```bash
cargo build --release --no-default-features --features saas
```

## Prerequisites

- Rust 1.93 or higher (or use Docker)

## Installation

1. Clone the repository:
```bash
git clone https://github.com/joshrandall8478/rus.git
cd rus
```

2. Copy and edit the environment file:
```bash
cp .env.standalone .env
# Edit .env and set JWT_SECRET
```

3. Build and run:
```bash
cargo build --release
cargo run --release
```

The application starts on `http://localhost:4001`.

### Docker Deployment

```bash
# Standalone (default)
docker build -t rus .
docker compose up --build

# SaaS mode
docker build --build-arg BUILD_MODE=saas -t rus-saas .
```

### Task Runner (just)

```bash
just check-standalone    # Compile-check standalone via Docker
just check-saas          # Compile-check saas via Docker
just check-all           # Check both modes
```

## Usage

### Web Interface

1. **Sign Up** - Create an account at `/signup.html`
2. **Log In** - Authenticate at `/login.html`
3. **Dashboard** - Manage your URLs at `/dashboard.html`:
   - Shorten new URLs
   - View click statistics and history
   - Generate QR codes
   - Rename URLs with custom names
   - Copy short URLs to clipboard
   - Delete URLs
4. **Admin** - Manage users and abuse reports at `/admin.html` (admin only)
5. **Report** - Report abusive URLs at `/report.html`

### API Endpoints

#### Public

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/register` | Register a new user (standalone only) |
| `POST` | `/api/login` | Login, returns JWT + refresh token (standalone only) |
| `POST` | `/api/token/refresh` | Refresh an expired JWT (standalone only) |
| `GET` | `/{short_code}` | Redirect to original URL |
| `POST` | `/api/report` | Report an abusive URL |

#### Protected (Bearer Token)

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/shorten` | Shorten a URL |
| `GET` | `/api/urls` | List user's URLs |
| `GET` | `/api/stats/{code}` | Get URL statistics |
| `GET` | `/api/stats/{code}/clicks` | Get click history |
| `DELETE` | `/api/urls/{code}` | Delete a URL |
| `PATCH` | `/api/urls/{code}/name` | Rename a URL |
| `GET` | `/api/qr/{code}` | Generate QR code (PNG) |
| `GET` | `/api/qr/{code}/svg` | Generate QR code (SVG) |
| `GET` | `/api/config` | Get public configuration |

#### Admin (Bearer Token, admin users only)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/admin/users` | List all users |
| `DELETE` | `/api/admin/users/{id}` | Delete a user |
| `PATCH` | `/api/admin/users/{id}/admin` | Toggle admin status |
| `GET` | `/api/admin/reports` | List abuse reports |
| `PATCH` | `/api/admin/reports/{id}` | Resolve an abuse report |

## Example Usage

### Using cURL

Register:
```bash
curl -X POST http://localhost:4001/api/register \
  -H "Content-Type: application/json" \
  -d '{"username":"test","password":"password123"}'
```

Login and save token:
```bash
TOKEN=$(curl -s -X POST http://localhost:4001/api/login \
  -H "Content-Type: application/json" \
  -d '{"username":"test","password":"password123"}' | jq -r '.token')
```

Shorten a URL:
```bash
curl -X POST http://localhost:4001/api/shorten \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"url":"https://github.com/joshrandall8478/rus"}'
```

Get your URLs:
```bash
curl http://localhost:4001/api/urls \
  -H "Authorization: Bearer $TOKEN"
```

## Project Structure

```
rus/
├── src/
│   ├── main.rs              # Entry point, route configuration
│   ├── config.rs            # Environment-based configuration
│   ├── db.rs                # Database connection and schema
│   ├── models.rs            # Data models and request/response types
│   ├── security.rs          # Password validation, account lockout
│   ├── auth/
│   │   ├── mod.rs
│   │   ├── jwt.rs           # JWT creation and validation
│   │   └── middleware.rs    # Auth middleware
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── auth.rs          # Registration, login (standalone)
│   │   ├── admin.rs         # User management (standalone)
│   │   ├── abuse.rs         # Abuse reporting
│   │   ├── pages.rs         # Static page serving
│   │   ├── saas_auth.rs     # Cookie-based auth (saas)
│   │   └── urls.rs          # URL CRUD, redirect, statistics
│   └── url/
│       ├── mod.rs
│       ├── shortener.rs     # Short code generation
│       └── qr.rs            # QR code generation
├── static/
│   ├── index.html           # Landing page
│   ├── login.html           # Login page
│   ├── signup.html          # Registration page
│   ├── dashboard.html       # URL management dashboard
│   ├── admin.html           # Admin panel
│   ├── report.html          # Abuse report form
│   ├── setup.html           # Initial setup page
│   ├── 404.html             # Custom 404 error page
│   ├── styles.css           # Global styles
│   └── auth.js              # Authentication utilities
├── oci-build/
│   ├── setup.nu             # Nushell build script
│   └── get-tags.nu          # Image tag derivation from git describe
├── data/
│   └── rus.db               # SQLite database (auto-created)
├── Cargo.toml
├── Dockerfile
├── compose.yml
├── justfile                 # Task runner recipes
├── .env.standalone          # Env template for standalone mode
└── .env.saas                # Env template for saas mode
```

## Environment Variables

### Shared (both modes)

| Variable | Description | Default |
|----------|-------------|---------|
| `DB_PATH` | Path to SQLite database file | `./data/rus.db` |
| `HOST` | Server bind address | `0.0.0.0` |
| `APP_PORT` | Server port | `4001` |
| `HOST_URL` | Public URL for shortened links | `http://localhost:4001` |
| `MAX_URL_LENGTH` | Maximum URL length | `2048` |
| `CLICK_RETENTION_DAYS` | Days to retain click history | `30` |
| `RUST_LOG` | Log level | `info` |

### Standalone only

| Variable | Description | Default |
|----------|-------------|---------|
| `JWT_SECRET` | Base64-encoded 32-byte secret for JWT signing | **Required** |
| `JWT_EXPIRY` | JWT expiry in hours | `1` |
| `REFRESH_TOKEN_EXPIRY` | Refresh token expiry in days | `7` |
| `ACCOUNT_LOCKOUT_ATTEMPTS` | Failed attempts before lockout | `5` |
| `ACCOUNT_LOCKOUT_DURATION` | Lockout duration in minutes | `30` |
| `ALLOW_REGISTRATION` | Allow public signups | `true` |

### SaaS only

| Variable | Description | Default |
|----------|-------------|---------|
| `SAAS_JWT_SECRET` | JWT secret for validating parent app tokens | **Required** |

## Database Schema

### users (standalone only)
- `userID` - Primary key
- `username` - Unique username
- `password` - Argon2id hashed password (legacy bcrypt hashes migrated on login)
- `is_admin` - Admin flag (0/1)
- `created_at` - Account creation timestamp

### urls
- `id` - Primary key
- `user_id` - Foreign key to users
- `original_url` - The original long URL
- `short_code` - Unique 6-character code (indexed)
- `name` - Optional custom name
- `clicks` - Click counter
- `created_at` - URL creation timestamp

### click_history
- `id` - Primary key
- `url_id` - Foreign key to urls
- `clicked_at` - Click timestamp

### refresh_tokens (standalone only)
- `id` - Primary key
- `user_id` - Foreign key to users
- `token` - Unique refresh token
- `expires_at` - Expiry timestamp

### login_attempts (standalone only)
- `id` - Primary key
- `username` - Attempted username
- `attempted_at` - Attempt timestamp
- `success` - Whether login succeeded (0/1)

### abuse_reports
- `id` - Primary key
- `short_code` - Reported URL code
- `reporter_email` - Optional reporter email
- `reason` - Report reason
- `description` - Optional description
- `status` - Report status (pending/resolved)
- `created_at`, `resolved_at`, `resolved_by`

## Technology Stack

- **[Actix-web](https://actix.rs/)** - High-performance web framework
- **[SQLite](https://www.sqlite.org/)** - Embedded database via rusqlite (bundled)
- **[jsonwebtoken](https://github.com/Keats/jsonwebtoken)** - JWT authentication
- **[Argon2id](https://en.wikipedia.org/wiki/Argon2)** - Password hashing (standalone)
- **[actix-governor](https://github.com/AaronErber/actix-governor)** - Rate limiting
- **[qrcode](https://github.com/kennytm/qrcode-rust)** - QR code generation
- **[Serde](https://serde.rs/)** - Serialization/deserialization
- **[Tokio](https://tokio.rs/)** - Async runtime

## Development

```bash
cargo run                    # Run in standalone mode (default)
cargo test                   # Run tests
cargo clippy                 # Lint
cargo fmt                    # Format

# SaaS mode
cargo run --no-default-features --features saas
cargo test --no-default-features --features saas
```

### Short Code Generation
- 6-character alphanumeric codes (A-Z, a-z, 0-9)
- 62^6 = ~56.8 billion possible combinations
- Collision detection ensures unique codes

## Security

- JWT-based authentication with short-lived tokens
- Argon2id password hashing (with transparent bcrypt migration on login)
- Refresh token rotation
- Account lockout after configurable failed attempts
- Rate limiting on API endpoints
- Protected API endpoints with user-scoped access
- SQL injection prevention via parameterized queries
- Foreign key enforcement enabled

## Contributing

Contributions are welcome! Feel free to:
- Report bugs
- Suggest features
- Submit pull requests

## License

This project is open source and available under the MIT License.

---

**Made with Rust**
