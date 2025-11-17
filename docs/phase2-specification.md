# Phase 2 Specification: SaaS URL Shortener Platform

## Overview
Multi-tenant SaaS platform offering URL shortening services with tiered pricing, team collaboration, custom domains, and enterprise-grade features.

## Business Model

### Pricing Tiers

| Feature | Free | Pro ($10/mo) | Business ($25/mo) | Enterprise (Custom) |
|---------|------|--------------|-------------------|---------------------|
| URLs | 25 | 500 | 5,000 | Unlimited |
| Clicks/month | 1,000 | 50,000 | 500,000 | Unlimited |
| Analytics Retention | 7 days | 90 days | 1 year | Unlimited |
| Custom Domains | 0 | 3 | 5 | Unlimited |
| Team Seats | 1 | 1 | 5 | Unlimited |
| API Access | Limited | Limited | Full | Full |
| API Requests/min | 60 | 100 | 1,000 | Custom |
| URL Creations/day | 1,000 | 1,000 | 10,000 | Custom |
| Priority Support | No | Email | Email + Chat | Dedicated |
| SLA | No | No | No | Yes |
| Webhooks | No | No | Yes | Yes |

### Billing
- **Payment Provider**: Stripe
- **Billing Cycles**: Monthly and Annual (2 months free on annual)
- **Annual Pricing**:
  - Pro: $100/year (save $20)
  - Business: $250/year (save $50)

## Authentication & Identity

### Authentication Methods
- Email/password with email verification
- Social login: Google, GitHub, Apple
- Passkeys (WebAuthn) support
- Multi-factor authentication (optional)

### User Management
- Email-based accounts (replacing username)
- Password reset flow
- Account deletion with data export (GDPR)
- Session management

## Team Features (Business & Enterprise)

### Roles & Permissions
- **Owner**: Full access, billing management, can delete team
- **Admin**: Manage team members, all URL operations, view audit logs
- **Member**: Create/edit/delete own URLs, view team URLs
- **Viewer**: Read-only access to team URLs and analytics

### Team URL Management
- Team-owned URLs (shared ownership)
- Personal URLs within team context
- Transfer URL ownership between members
- Bulk operations for admins

### Audit Logging
- Track all team actions (URL creation, deletion, permission changes)
- Filterable audit log dashboard
- Export audit logs (CSV/JSON)
- Retention based on tier (Business: 90 days, Enterprise: unlimited)

## Custom Domains

### Features
- User brings own domain (CNAME setup)
- DNS verification process
- Automatic SSL certificate provisioning (managed by platform)
- Domain health monitoring
- Limits per tier:
  - Pro: 3 domains
  - Business: 5 domains
  - Enterprise: Unlimited

### Setup Flow
1. User adds domain in dashboard
2. System provides CNAME record to configure
3. User configures DNS at their registrar
4. System verifies DNS propagation
5. SSL certificate auto-provisioned
6. Domain becomes active

## API Platform (Business & Enterprise)

### API Keys
- Multiple API keys per account
- Key rotation support
- Scoped permissions per key
- Usage tracking per key
- Revocation capability

### Rate Limiting (Redis-backed)
- Per-user/per-key limits
- Sliding window algorithm
- Rate limit headers in responses
- Graceful degradation
- Tier-based limits enforced

### Webhooks
- Events: URL created, URL deleted, click threshold reached
- Retry logic with exponential backoff
- Webhook signature verification
- Event history and logs
- Multiple endpoints per account

### Documentation
- OpenAPI/Swagger specification
- Interactive API explorer
- Code examples in multiple languages
- Authentication guide
- Rate limiting documentation

### SDKs
- JavaScript/TypeScript
- Python
- Go (optional, given Rust backend)
- Published to npm, PyPI

## Infrastructure

### Cloud Providers
- AWS
- Google Cloud Platform
- DigitalOcean
- Multi-cloud deployment support

### Database
- PostgreSQL (migration from SQLite)
- Connection pooling
- Read replicas for scaling
- Automated backups
- Point-in-time recovery

### Caching & Rate Limiting
- Redis cluster
- Session storage
- Rate limit counters
- URL redirect caching
- API response caching

### Multi-Region Deployment
- Geographic distribution (US, EU, Asia-Pacific)
- Latency-based routing
- Data residency compliance
- Regional failover
- CDN for static assets

### Observability

#### Logging
- Structured JSON logs
- Centralized aggregation
- Log levels (debug, info, warn, error)
- Request tracing IDs
- PII redaction

#### Metrics
- Application metrics (request latency, error rates, throughput)
- Business metrics (signups, conversions, churn)
- Infrastructure metrics (CPU, memory, disk)
- Prometheus/Grafana dashboards
- Custom alerting thresholds

#### Distributed Tracing
- OpenTelemetry integration
- Request flow visualization
- Performance bottleneck identification
- Cross-service tracing
- Sampling strategies

## Compliance & Legal

### GDPR Compliance
- Right to access (data export)
- Right to deletion (account removal)
- Data portability
- Consent management
- Data processing agreements

### Cookie Consent
- Cookie banner with preferences
- Essential vs. optional cookies
- Consent storage
- Preference center
- Compliance with ePrivacy directive

### Data Retention Policies
- Click history: Based on tier
- Account data: 30 days after deletion
- Audit logs: Based on tier
- Backup retention: 90 days
- Automated cleanup jobs

### Legal Pages
- Terms of Service
- Privacy Policy
- Cookie Policy
- Acceptable Use Policy
- Data Processing Agreement (for Business/Enterprise)

### SOC 2 Compliance
- Security controls documentation
- Annual audits
- Penetration testing
- Vulnerability management
- Incident response procedures

### Licensing
- Open Source Maintenance License
- Clear attribution requirements
- Commercial use terms

## Email Communications

### Transactional Emails
- Email verification
- Password reset
- Billing receipts and invoices
- Usage alerts (approaching limits)
- Team invitations
- Security notifications (new login, password change)

### Email Service Provider
- SendGrid or Postmark (easy integration)
- High deliverability
- Template management
- Analytics and tracking

### Email Templates
- Rust-themed branding (black and orange)
- Responsive HTML design
- Plain text fallbacks
- Consistent styling across all emails
- Unsubscribe links (where applicable)

## Security Enhancements

### Additional Security Features
- IP allowlisting (Enterprise)
- Session management dashboard
- Login history
- Suspicious activity alerts
- CAPTCHA for signup (optional)
- Brute force protection (inherited from Phase 1)

### Data Security
- Encryption at rest
- Encryption in transit (TLS 1.3)
- Regular security audits
- Dependency vulnerability scanning
- Secret management (HashiCorp Vault or cloud KMS)

## Frontend Enhancements

### New Pages
- Pricing page with tier comparison
- Team management dashboard
- Billing and subscription management
- API key management
- Custom domain configuration
- Audit log viewer
- Account settings (profile, security, notifications)
- Public status page

### Dashboard Improvements
- Usage quota visualization
- Upgrade prompts when approaching limits
- Team switcher (for users in multiple teams)
- Advanced analytics with more visualizations
- Export functionality (CSV, PDF reports)

## Migration Path

### From Phase 1
- Database schema migration (SQLite → PostgreSQL)
- User migration (username → email)
- Environment variable updates
- Docker Compose → Kubernetes/cloud-native
- Single instance → multi-region

### Data Migration Strategy
- Zero-downtime migration
- Backward compatibility period
- Rollback procedures
- Data validation checks
- User communication plan

## Success Metrics

### Key Performance Indicators
- Monthly Recurring Revenue (MRR)
- Customer Acquisition Cost (CAC)
- Lifetime Value (LTV)
- Churn rate
- Free-to-paid conversion rate
- API usage growth
- Uptime percentage (target: 99.9%)

### Monitoring
- Real-time dashboard for business metrics
- Automated alerts for anomalies
- Weekly/monthly reports
- Cohort analysis
- Feature usage tracking
