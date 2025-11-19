# rus
**Rust URL Shortener** - A fast, secure, and elegant URL shortening web application built with Rust 🦀

![URL Shortener Homepage](/assets/screenshot.png)

## Features

- 🔒 **JWT Authentication** - Secure user registration and login with bcrypt password hashing
- 💾 **SQLite Persistence** - Reliable data storage with SQLite database
- 📊 **Click Tracking** - Monitor how many times each shortened URL is accessed
- ✏️ **Custom Names** - Give your shortened URLs memorable names
- 🗑️ **URL Management** - Delete URLs you no longer need
- 🚀 **Fast Performance** - Built with Rust and Actix-web for maximum speed
- 🎨 **Modern Dark UI** - Beautiful, responsive web interface with Rust-themed colors
- 🦀 **Custom 404 Page** - Friendly error page with panicked crab when short codes aren't found
- 🐳 **Docker Support** - Easy deployment with Docker Compose

## Prerequisites

- Rust 1.91.0 or higher
- Cargo (comes with Rust)

Or use Docker for containerized deployment.

## Installation

1. Clone the repository:
```bash
git clone https://github.com/joshrandall8478/rus.git
cd rus
```

2. Create a `.env` file with your JWT secret:
```bash
JWT_SECRET=<base64-encoded-32-bytes>
```

3. Build and run the project:
```bash
cargo build --release
cargo run --release
```

The application will start on `http://localhost:8080`

### Docker Deployment

```bash
docker compose up --build
```

## Usage

### Web Interface

1. **Sign Up** - Create an account at `/signup.html`
2. **Log In** - Authenticate at `/login.html`
3. **Dashboard** - Manage your URLs at `/dashboard.html`:
   - Shorten new URLs
   - View click statistics
   - Rename URLs with custom names
   - Copy short URLs to clipboard
   - Delete URLs you no longer need

### API Endpoints

#### Public Endpoints

**Register a new user:**
```bash
POST /api/register
Content-Type: application/json

{
  "username": "myuser",
  "password": "mypassword"
}
```

**Login:**
```bash
POST /api/login
Content-Type: application/json

{
  "username": "myuser",
  "password": "mypassword"
}
```

**Response:**
```json
{
  "token": "eyJhbGciOiJIUzI1NiIs..."
}
```

**Redirect to Original URL:**
```bash
GET /{short_code}
```
Redirects to the original URL and increments the click counter.

#### Protected Endpoints (Require Bearer Token)

**Shorten a URL:**
```bash
POST /api/shorten
Content-Type: application/json
Authorization: Bearer {TOKEN}

{
  "url": "https://example.com/very/long/url"
}
```

**Response:**
```json
{
  "short_code": "abc123",
  "short_url": "http://localhost:8080/abc123",
  "original_url": "https://example.com/very/long/url"
}
```

**Get all user URLs:**
```bash
GET /api/urls
Authorization: Bearer {TOKEN}
```

**Get URL statistics:**
```bash
GET /api/stats/{short_code}
Authorization: Bearer {TOKEN}
```

**Response:**
```json
{
  "original_url": "https://example.com/very/long/url",
  "short_code": "abc123",
  "name": "My Link",
  "clicks": 42
}
```

**Delete a URL:**
```bash
DELETE /api/urls/{short_code}
Authorization: Bearer {TOKEN}
```

**Rename a URL:**
```bash
PATCH /api/urls/{short_code}/name
Content-Type: application/json
Authorization: Bearer {TOKEN}

{
  "name": "My Custom Name"
}
```

## Example Usage

### Using cURL

Register:
```bash
curl -X POST http://localhost:8080/api/register \
  -H "Content-Type: application/json" \
  -d '{"username":"test","password":"password123"}'
```

Login and save token:
```bash
TOKEN=$(curl -s -X POST http://localhost:8080/api/login \
  -H "Content-Type: application/json" \
  -d '{"username":"test","password":"password123"}' | jq -r '.token')
```

Shorten a URL:
```bash
curl -X POST http://localhost:8080/api/shorten \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"url":"https://github.com/joshrandall8478/rus"}'
```

Get your URLs:
```bash
curl http://localhost:8080/api/urls \
  -H "Authorization: Bearer $TOKEN"
```

## Project Structure

```
rus/
├── src/
│   └── main.rs          # Main application code
├── static/
│   ├── index.html       # Landing page
│   ├── login.html       # Login page
│   ├── signup.html      # Registration page
│   ├── dashboard.html   # URL management dashboard
│   ├── 404.html         # Custom 404 error page
│   ├── styles.css       # Global styles
│   └── auth.js          # Authentication utilities
├── data/
│   └── rus.db           # SQLite database (auto-created)
├── Cargo.toml           # Rust dependencies
├── compose.yml          # Docker Compose configuration
├── Dockerfile           # Docker build configuration
└── README.md            # This file
```

## Technology Stack

- **[Actix-web](https://actix.rs/)** - High-performance web framework for Rust
- **[SQLite](https://www.sqlite.org/)** - Embedded relational database via rusqlite
- **[JSON Web Tokens](https://jwt.io/)** - Secure authentication via jsonwebtoken
- **[bcrypt](https://en.wikipedia.org/wiki/Bcrypt)** - Secure password hashing
- **[Serde](https://serde.rs/)** - Serialization/deserialization framework
- **[Tokio](https://tokio.rs/)** - Async runtime for Rust

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `JWT_SECRET` | Base64-encoded 32-byte secret for JWT signing | **Required** |
| `DB_PATH` | Path to SQLite database file | `./data/rus.db` |
| `HOST` | Server bind address | `0.0.0.0` |
| `PORT` | Server port | `8080` |

## Database Schema

**users table:**
- `userID` - Primary key
- `username` - Unique username
- `password` - bcrypt hashed password
- `created_at` - Account creation timestamp

**urls table:**
- `id` - Primary key
- `user_id` - Foreign key to users
- `original_url` - The original long URL
- `short_code` - Unique 6-character code (indexed)
- `name` - Optional custom name
- `clicks` - Click counter
- `created_at` - URL creation timestamp

## Development

### Run in development mode:
```bash
cargo run
```

### Run tests:
```bash
cargo test
```

### Lint code:
```bash
cargo clippy
```

### Format code:
```bash
cargo fmt
```

### Build for production:
```bash
cargo build --release
```

The optimized binary will be available at `./target/release/rus`

## Features in Detail

### Authentication
- JWT tokens with 24-hour expiry
- bcrypt password hashing (cost factor 12)
- Tokens stored in localStorage on frontend

### Short Code Generation
- 6-character alphanumeric codes (A-Z, a-z, 0-9)
- 62^6 = ~56.8 billion possible combinations
- Collision detection ensures unique codes

### Click Tracking
- Each redirect increments a counter
- View statistics in dashboard or via API
- Useful for analytics and monitoring

### Error Handling
- Validates URLs and authentication
- Custom 404 page with friendly error message
- Returns proper HTTP status codes
- User-friendly error messages

## Security Features

- ✅ JWT-based authentication
- ✅ bcrypt password hashing
- ✅ Protected API endpoints
- ✅ User-scoped URL management
- ✅ SQL injection prevention via parameterized queries

### Production Considerations

For production deployment, also consider:
- Implementing rate limiting
- Setting up HTTPS with TLS
- Adding CORS configuration
- Configuring proper logging and monitoring
- Using connection pooling for the database

## Contributing

Contributions are welcome! Feel free to:
- Report bugs
- Suggest features
- Submit pull requests

## License

This project is open source and available under the MIT License.

## Acknowledgments

- Built with ❤️ using Rust
- Inspired by URL shortening services like bit.ly and TinyURL
- Special thanks to the Rust and Actix-web communities

---

**Made with 🦀 and Rust**
