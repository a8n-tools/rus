# rus
**Rust URL Shortener** - A fast, simple, and elegant URL shortening web application built with Rust 🦀

![URL Shortener Homepage](https://github.com/user-attachments/assets/b312c760-d373-4e21-9887-28dc9fab1a18)

## Features

- ✨ **Clean and Modern UI** - Beautiful, responsive web interface with a gradient design
- 🚀 **Fast Performance** - Built with Rust and Actix-web for maximum speed
- 🔗 **URL Shortening** - Convert long URLs into short, shareable links
- 📊 **Click Tracking** - Monitor how many times each shortened URL is accessed
- 🎯 **Simple API** - RESTful API endpoints for programmatic access
- 💾 **In-Memory Storage** - Quick access with HashMap-based storage
- 🦀 **Pure Rust** - Written entirely in Rust for safety and performance

## Prerequisites

- Rust 1.70 or higher
- Cargo (comes with Rust)

## Installation

1. Clone the repository:
```bash
git clone https://github.com/joshrandall8478/rus.git
cd rus
```

2. Build the project:
```bash
cargo build --release
```

3. Run the application:
```bash
cargo run --release
```

The application will start on `http://localhost:8080`

## Usage

### Web Interface

1. Open your browser and navigate to `http://localhost:8080`
2. Enter a long URL in the input field
3. Click "Shorten URL"
4. Copy the shortened URL and share it!

![URL Shortening Result](https://github.com/user-attachments/assets/3ea8be05-4af1-497c-ae28-e3b21633818e)

### API Endpoints

#### Shorten a URL

```bash
POST /api/shorten
Content-Type: application/json

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

#### Redirect to Original URL

```bash
GET /{short_code}
```

Redirects to the original URL and increments the click counter.

#### Get URL Statistics

```bash
GET /api/stats/{short_code}
```

**Response:**
```json
{
  "original_url": "https://example.com/very/long/url",
  "short_code": "abc123",
  "clicks": 42
}
```

## Example Usage

### Using cURL

Shorten a URL:
```bash
curl -X POST http://localhost:8080/api/shorten \
  -H "Content-Type: application/json" \
  -d '{"url":"https://github.com/joshrandall8478/rus"}'
```

Get statistics:
```bash
curl http://localhost:8080/api/stats/abc123
```

### Using the short URL

Simply visit the shortened URL in your browser:
```
http://localhost:8080/abc123
```

You'll be automatically redirected to the original URL.

## Project Structure

```
rus/
├── src/
│   └── main.rs          # Main application code
├── static/
│   └── index.html       # Web interface
├── Cargo.toml           # Rust dependencies
└── README.md            # This file
```

## Technology Stack

- **[Actix-web](https://actix.rs/)** - High-performance web framework for Rust
- **[Serde](https://serde.rs/)** - Serialization/deserialization framework
- **[Tokio](https://tokio.rs/)** - Async runtime for Rust
- **[Rand](https://rust-random.github.io/rand/)** - Random number generation for short codes

## Development

### Run in development mode:
```bash
cargo run
```

### Run tests:
```bash
cargo test
```

### Build for production:
```bash
cargo build --release
```

The optimized binary will be available at `./target/release/rus`

## Configuration

By default, the application runs on `localhost:8080`. To change the host or port, modify the `.bind()` call in `src/main.rs`:

```rust
.bind(("127.0.0.1", 8080))?
```

## Features in Detail

### Short Code Generation
- 6-character alphanumeric codes (A-Z, a-z, 0-9)
- 62^6 = ~56.8 billion possible combinations
- Collision detection ensures unique codes

### Click Tracking
- Each redirect increments a counter
- View statistics via the API endpoint
- Useful for analytics and monitoring

### Error Handling
- Validates empty URLs
- Returns proper HTTP status codes
- User-friendly error messages

## Security Considerations

⚠️ **Note:** This is a demonstration project. For production use, consider:
- Adding authentication for API endpoints
- Implementing rate limiting
- Using persistent storage (database)
- Adding URL validation and sanitization
- Implementing HTTPS
- Adding CORS configuration
- Setting up proper logging and monitoring

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

