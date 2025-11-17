# Implementation Blueprint: Rust URL Shortener

## High-Level Architecture

### Phase 1: Self-Hosted Production Ready
```
┌─────────────────────────────────────────────────────────┐
│                    Docker Container                      │
│  ┌─────────────┐    ┌──────────────┐    ┌───────────┐ │
│  │   Actix-Web │    │   Business   │    │  SQLite   │ │
│  │   (HTTP)    │───▶│    Logic     │───▶│    DB     │ │
│  └─────────────┘    └──────────────┘    └───────────┘ │
│         │                   │                           │
│         ▼                   ▼                           │
│  ┌─────────────┐    ┌──────────────┐                   │
│  │   Static    │    │  QR Code     │                   │
│  │   Files     │    │  Generator   │                   │
│  └─────────────┘    └──────────────┘                   │
└─────────────────────────────────────────────────────────┘
```

### Phase 2: SaaS Multi-Tenant
```
┌─────────────────────────────────────────────────────────────────┐
│                         Load Balancer                           │
└─────────────────────────┬───────────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────────┐
│                    Application Cluster                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐      │
│  │  App 1   │  │  App 2   │  │  App 3   │  │  App N   │      │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘      │
└─────────────────────────┬───────────────────────────────────────┘
                          │
        ┌─────────────────┼─────────────────┐
        │                 │                 │
        ▼                 ▼                 ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│  PostgreSQL  │  │    Redis     │  │   S3/Blob    │
│   (Primary)  │  │   Cluster    │  │   Storage    │
└──────────────┘  └──────────────┘  └──────────────┘
        │
        ▼
┌──────────────┐
│   Read       │
│   Replicas   │
└──────────────┘
```

---

## Decomposition: Level 1 (Major Milestones)

### Phase 1 Milestones
1. **M1.1**: Backend Security & Validation Hardening
2. **M1.2**: Authentication System Overhaul (Refresh Tokens)
3. **M1.3**: Click Analytics & History System
4. **M1.4**: QR Code Generation Service
5. **M1.5**: Frontend Redesign (Rust Theme)
6. **M1.6**: Dashboard Enhancements (Charts, Sorting, Filtering)
7. **M1.7**: Docker & Deployment Configuration
8. **M1.8**: Testing & Documentation

### Phase 2 Milestones
1. **M2.1**: Database Migration (SQLite → PostgreSQL)
2. **M2.2**: Multi-Tenant Architecture
3. **M2.3**: Email-Based Authentication
4. **M2.4**: Social Login Integration
5. **M2.5**: Passkey Support
6. **M2.6**: Subscription & Billing (Stripe)
7. **M2.7**: Team Management System
8. **M2.8**: Custom Domain Support
9. **M2.9**: API Platform (Keys, Rate Limiting, Webhooks)
10. **M2.10**: Observability (Logging, Metrics, Tracing)
11. **M2.11**: Compliance & Legal Features
12. **M2.12**: Email Service Integration
13. **M2.13**: Multi-Region Deployment
14. **M2.14**: SDK Development

---

## Decomposition: Level 2 (Features)

### M1.1: Backend Security & Validation Hardening
- F1.1.1: Environment variable configuration system
- F1.1.2: Password complexity validation
- F1.1.3: Account lockout mechanism
- F1.1.4: URL input validation (schemes, length, patterns)
- F1.1.5: Health check endpoint

### M1.2: Authentication System Overhaul
- F1.2.1: Refresh token generation and storage
- F1.2.2: Token refresh endpoint
- F1.2.3: Token rotation on refresh
- F1.2.4: Expired token cleanup

### M1.3: Click Analytics & History System
- F1.3.1: Click history table and tracking
- F1.3.2: Click history API endpoint
- F1.3.3: Automatic cleanup based on retention
- F1.3.4: Aggregation queries (daily, weekly)

### M1.4: QR Code Generation Service
- F1.4.1: QR code library integration
- F1.4.2: PNG generation with Rust logo
- F1.4.3: SVG generation with Rust logo
- F1.4.4: QR code API endpoint

### M1.5: Frontend Redesign
- F1.5.1: Color scheme update (black/orange)
- F1.5.2: Uniform navigation component
- F1.5.3: Mobile responsive layout
- F1.5.4: Landing page redesign
- F1.5.5: Auth pages redesign

### M1.6: Dashboard Enhancements
- F1.6.1: URL sorting functionality
- F1.6.2: URL filtering functionality
- F1.6.3: Click history visualization (charts)
- F1.6.4: QR code download integration
- F1.6.5: HOST_URL integration
- F1.6.6: Refresh token handling in frontend

### M1.7: Docker Configuration
- F1.7.1: Environment variable documentation
- F1.7.2: Docker Compose updates
- F1.7.3: Health check configuration
- F1.7.4: Volume and persistence setup

### M1.8: Testing & Documentation
- F1.8.1: API endpoint testing
- F1.8.2: Security validation testing
- F1.8.3: Updated README
- F1.8.4: API documentation

---

## Decomposition: Level 3 (Tasks)

### F1.1.1: Environment variable configuration system
- T1: Create Config struct with all fields
- T2: Implement from_env() with defaults
- T3: Update AppState to include Config
- T4: Wire config into main()
- T5: Add startup logging for config values

### F1.1.2: Password complexity validation
- T1: Create validate_password() function
- T2: Check minimum length (8 chars)
- T3: Check for uppercase letter
- T4: Check for number
- T5: Check for special character
- T6: Integrate into register endpoint
- T7: Return descriptive error messages

### F1.1.3: Account lockout mechanism
- T1: Create login_attempts table
- T2: Create record_login_attempt() function
- T3: Create is_account_locked() function
- T4: Integrate into login endpoint
- T5: Return appropriate error on lockout

### F1.1.4: URL input validation
- T1: Add url crate dependency
- T2: Create validate_url() function
- T3: Check URL length against max
- T4: Parse and validate scheme (http/https only)
- T5: Block dangerous patterns
- T6: Integrate into shorten_url endpoint

### F1.1.5: Health check endpoint
- T1: Create HealthResponse struct
- T2: Add uptime tracking to AppState
- T3: Implement health_check handler
- T4: Register /health route

---

## Decomposition: Level 4 (Implementation Steps)

Now breaking down into right-sized implementation chunks that can be safely implemented incrementally:

### Chunk 1: Foundation - Config System
**Scope**: Create centralized configuration management
**Files**: src/main.rs
**Dependencies**: None
**Output**: Config struct, from_env(), startup logging

### Chunk 2: Password Validation
**Scope**: Add password complexity requirements
**Files**: src/main.rs
**Dependencies**: Chunk 1
**Output**: validate_password(), integrated into register

### Chunk 3: Login Attempts Tracking
**Scope**: Database table and recording logic
**Files**: src/main.rs, init.sql
**Dependencies**: Chunk 2
**Output**: Table schema, record function

### Chunk 4: Account Lockout Logic
**Scope**: Check and enforce lockout
**Files**: src/main.rs
**Dependencies**: Chunk 3
**Output**: is_account_locked(), integrated into login

### Chunk 5: URL Validation
**Scope**: Validate URL input
**Files**: src/main.rs, Cargo.toml
**Dependencies**: Chunk 1
**Output**: validate_url(), integrated into shorten

### Chunk 6: Health Check Endpoint
**Scope**: Add monitoring endpoint
**Files**: src/main.rs
**Dependencies**: Chunk 1
**Output**: /health endpoint with uptime

### Chunk 7: Refresh Token Schema
**Scope**: Database table for refresh tokens
**Files**: src/main.rs, init.sql
**Dependencies**: Chunk 6
**Output**: Table schema, structs

### Chunk 8: Refresh Token Generation
**Scope**: Create and store refresh tokens
**Files**: src/main.rs
**Dependencies**: Chunk 7
**Output**: generate_refresh_token(), storage logic

### Chunk 9: Auth Response Update
**Scope**: Return refresh token in auth responses
**Files**: src/main.rs
**Dependencies**: Chunk 8
**Output**: Updated register/login responses

### Chunk 10: Token Refresh Endpoint
**Scope**: Implement /api/refresh
**Files**: src/main.rs
**Dependencies**: Chunk 9
**Output**: refresh_token handler, token rotation

### Chunk 11: Click History Schema
**Scope**: Database table for click tracking
**Files**: src/main.rs, init.sql
**Dependencies**: Chunk 10
**Output**: Table schema, indexes

### Chunk 12: Click Recording
**Scope**: Record clicks in redirect handler
**Files**: src/main.rs
**Dependencies**: Chunk 11
**Output**: INSERT into click_history

### Chunk 13: Click History Cleanup
**Scope**: Automatic old click removal
**Files**: src/main.rs
**Dependencies**: Chunk 12
**Output**: cleanup_old_clicks(), periodic trigger

### Chunk 14: Click History API
**Scope**: Endpoint to retrieve history
**Files**: src/main.rs
**Dependencies**: Chunk 13
**Output**: /api/urls/{code}/clicks endpoint

### Chunk 15: QR Code Dependencies
**Scope**: Add QR and image libraries
**Files**: Cargo.toml
**Dependencies**: Chunk 14
**Output**: Dependencies added

### Chunk 16: QR PNG Generation
**Scope**: Generate QR code as PNG
**Files**: src/main.rs
**Dependencies**: Chunk 15
**Output**: generate_qr_code_png()

### Chunk 17: QR Logo Branding
**Scope**: Add Rust logo to QR center
**Files**: src/main.rs
**Dependencies**: Chunk 16
**Output**: Logo overlay in PNG

### Chunk 18: QR SVG Generation
**Scope**: Generate QR code as SVG
**Files**: src/main.rs
**Dependencies**: Chunk 17
**Output**: generate_qr_code_svg()

### Chunk 19: QR API Endpoint
**Scope**: Serve QR codes via API
**Files**: src/main.rs
**Dependencies**: Chunk 18
**Output**: /api/urls/{code}/qr/{format}

### Chunk 20: Config API Endpoint
**Scope**: Public config for frontend
**Files**: src/main.rs
**Dependencies**: Chunk 19
**Output**: /api/config endpoint

### Chunk 21: Frontend Color Variables
**Scope**: CSS custom properties for theme
**Files**: static/styles.css
**Dependencies**: Chunk 20
**Output**: CSS variables for black/orange

### Chunk 22: Global Styles Update
**Scope**: Apply Rust theme globally
**Files**: static/styles.css
**Dependencies**: Chunk 21
**Output**: Updated backgrounds, colors

### Chunk 23: Navigation Component
**Scope**: Uniform navbar HTML/CSS
**Files**: static/styles.css
**Dependencies**: Chunk 22
**Output**: Navbar styles

### Chunk 24: Index Page Redesign
**Scope**: Landing page with new theme
**Files**: static/index.html
**Dependencies**: Chunk 23
**Output**: Themed landing page

### Chunk 25: Login Page Redesign
**Scope**: Login form with new theme
**Files**: static/login.html
**Dependencies**: Chunk 24
**Output**: Themed login page

### Chunk 26: Signup Page Redesign
**Scope**: Registration with password hints
**Files**: static/signup.html
**Dependencies**: Chunk 25
**Output**: Themed signup with validation hints

### Chunk 27: Auth.js Refresh Token Storage
**Scope**: Store refresh token in localStorage
**Files**: static/auth.js
**Dependencies**: Chunk 26
**Output**: Updated saveAuth(), getRefreshToken()

### Chunk 28: Auth.js Token Refresh Logic
**Scope**: Automatic token refresh
**Files**: static/auth.js
**Dependencies**: Chunk 27
**Output**: refreshToken(), auto-refresh on 401

### Chunk 29: Dashboard Base Redesign
**Scope**: Dashboard with new theme
**Files**: static/dashboard.html
**Dependencies**: Chunk 28
**Output**: Themed dashboard structure

### Chunk 30: Dashboard URL List Update
**Scope**: Display created_at, use HOST_URL
**Files**: static/dashboard.html
**Dependencies**: Chunk 29
**Output**: Enhanced URL cards

### Chunk 31: Dashboard Sorting
**Scope**: Sort URLs by date/clicks/name
**Files**: static/dashboard.html
**Dependencies**: Chunk 30
**Output**: Sort controls and logic

### Chunk 32: Dashboard Filtering
**Scope**: Filter URLs by search term
**Files**: static/dashboard.html
**Dependencies**: Chunk 31
**Output**: Filter input and logic

### Chunk 33: Chart.js Integration
**Scope**: Add charting library
**Files**: static/dashboard.html
**Dependencies**: Chunk 32
**Output**: Chart.js CDN include

### Chunk 34: Click History Modal
**Scope**: Modal to view click analytics
**Files**: static/dashboard.html
**Dependencies**: Chunk 33
**Output**: Modal HTML structure

### Chunk 35: Line Chart Visualization
**Scope**: Clicks over time line chart
**Files**: static/dashboard.html
**Dependencies**: Chunk 34
**Output**: Line chart rendering

### Chunk 36: Bar Chart Visualization
**Scope**: Clicks by day bar chart
**Files**: static/dashboard.html
**Dependencies**: Chunk 35
**Output**: Bar chart rendering

### Chunk 37: Table Visualization
**Scope**: Click history table view
**Files**: static/dashboard.html
**Dependencies**: Chunk 36
**Output**: Tabular data display

### Chunk 38: QR Code Modal
**Scope**: Modal for QR display/download
**Files**: static/dashboard.html
**Dependencies**: Chunk 37
**Output**: QR modal with download buttons

### Chunk 39: Mobile Responsive Layout
**Scope**: Media queries for mobile
**Files**: static/styles.css
**Dependencies**: Chunk 38
**Output**: Responsive breakpoints

### Chunk 40: Docker Environment Variables
**Scope**: Update compose with new env vars
**Files**: compose.yml, .env.example
**Dependencies**: Chunk 39
**Output**: Complete env var configuration

### Chunk 41: Docker Health Check
**Scope**: Container health monitoring
**Files**: compose.yml
**Dependencies**: Chunk 40
**Output**: Health check in compose

### Chunk 42: Integration Testing
**Scope**: Test complete flow
**Files**: Manual/script testing
**Dependencies**: Chunk 41
**Output**: Verified working system

### Chunk 43: Documentation Update
**Scope**: Update README and CLAUDE.md
**Files**: README.md, CLAUDE.md
**Dependencies**: Chunk 42
**Output**: Updated documentation

---

## Verification: Step Sizing

Each chunk is:
- **Small enough**: 1-3 files modified, single responsibility
- **Large enough**: Adds measurable functionality
- **Self-contained**: Can be tested independently
- **Builds on previous**: No orphaned code
- **Integrates immediately**: Wired into existing system

The chunks follow these principles:
1. Backend before frontend (API available first)
2. Schema before logic (database ready)
3. Core before enhancement (basic flow works)
4. Security integrated throughout (not bolted on)
5. Each chunk leaves system in working state
