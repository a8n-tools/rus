# Chunk 41: Integration Testing

## Context
Docker configuration complete. Now test the complete application flow.

## Goal
Create a test script to verify all Phase 1 features work correctly.

## Prompt

```text
I have Docker configured. Now create integration test script.

Create test-phase1.sh in project root:

```bash
#!/bin/bash

# Phase 1 Integration Tests for RUS
# Run this after starting the application

set -e

BASE_URL="${BASE_URL:-http://localhost:8080}"
USERNAME="testuser_$(date +%s)"
PASSWORD="TestP@ssw0rd!"

echo "🧪 RUS Phase 1 Integration Tests"
echo "================================="
echo "Base URL: $BASE_URL"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

pass() {
    echo -e "${GREEN}✓ PASS${NC}: $1"
}

fail() {
    echo -e "${RED}✗ FAIL${NC}: $1"
    exit 1
}

# Test 1: Health Check
echo "Test 1: Health Check"
HEALTH=$(curl -s "$BASE_URL/health")
echo "$HEALTH" | grep -q '"status":"healthy"' && pass "Health check returns healthy" || fail "Health check failed"

# Test 2: Get Config
echo "Test 2: Public Config"
CONFIG=$(curl -s "$BASE_URL/api/config")
echo "$CONFIG" | grep -q '"host_url"' && pass "Config endpoint accessible" || fail "Config endpoint failed"

# Test 3: Registration with Weak Password (should fail)
echo "Test 3: Password Validation"
WEAK_PASS=$(curl -s -X POST "$BASE_URL/api/register" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USERNAME\",\"password\":\"weak\"}")
echo "$WEAK_PASS" | grep -q '"error"' && pass "Weak password rejected" || fail "Weak password accepted"

# Test 4: Registration with Strong Password
echo "Test 4: User Registration"
REGISTER=$(curl -s -X POST "$BASE_URL/api/register" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\"}")
echo "$REGISTER" | grep -q '"token"' || fail "Registration failed"
echo "$REGISTER" | grep -q '"refresh_token"' && pass "Registration returns refresh token" || fail "No refresh token"
TOKEN=$(echo "$REGISTER" | grep -o '"token":"[^"]*"' | cut -d'"' -f4)
REFRESH=$(echo "$REGISTER" | grep -o '"refresh_token":"[^"]*"' | cut -d'"' -f4)

# Test 5: Login
echo "Test 5: User Login"
LOGIN=$(curl -s -X POST "$BASE_URL/api/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\"}")
echo "$LOGIN" | grep -q '"refresh_token"' && pass "Login returns refresh token" || fail "Login failed"

# Test 6: Token Refresh
echo "Test 6: Token Refresh"
REFRESH_RESP=$(curl -s -X POST "$BASE_URL/api/refresh" \
    -H "Content-Type: application/json" \
    -d "{\"refresh_token\":\"$REFRESH\"}")
echo "$REFRESH_RESP" | grep -q '"token"' && pass "Token refresh works" || fail "Token refresh failed"
NEW_TOKEN=$(echo "$REFRESH_RESP" | grep -o '"token":"[^"]*"' | cut -d'"' -f4)
TOKEN="$NEW_TOKEN"

# Test 7: URL Validation (invalid scheme)
echo "Test 7: URL Validation - Invalid Scheme"
INVALID_URL=$(curl -s -X POST "$BASE_URL/api/shorten" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d '{"url":"ftp://example.com"}')
echo "$INVALID_URL" | grep -q '"error"' && pass "Invalid scheme rejected" || fail "Invalid scheme accepted"

# Test 8: URL Validation (dangerous pattern)
echo "Test 8: URL Validation - Dangerous Pattern"
DANGEROUS=$(curl -s -X POST "$BASE_URL/api/shorten" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d '{"url":"https://example.com/javascript:alert(1)"}')
echo "$DANGEROUS" | grep -q '"error"' && pass "Dangerous pattern rejected" || fail "Dangerous pattern accepted"

# Test 9: Shorten Valid URL
echo "Test 9: Shorten Valid URL"
SHORTEN=$(curl -s -X POST "$BASE_URL/api/shorten" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d '{"url":"https://www.rust-lang.org"}')
echo "$SHORTEN" | grep -q '"short_code"' || fail "Shortening failed"
SHORT_CODE=$(echo "$SHORTEN" | grep -o '"short_code":"[^"]*"' | cut -d'"' -f4)
pass "URL shortened: $SHORT_CODE"

# Test 10: Get User URLs
echo "Test 10: Get User URLs"
URLS=$(curl -s "$BASE_URL/api/urls" \
    -H "Authorization: Bearer $TOKEN")
echo "$URLS" | grep -q '"created_at"' && pass "URLs include created_at" || fail "Missing created_at"

# Test 11: Redirect and Track Click
echo "Test 11: Redirect Click Tracking"
curl -s -I "$BASE_URL/$SHORT_CODE" | grep -q "302\|301" && pass "Redirect works" || fail "Redirect failed"

# Test 12: Get Click History
echo "Test 12: Click History API"
sleep 1  # Allow click to be recorded
CLICKS=$(curl -s "$BASE_URL/api/urls/$SHORT_CODE/clicks" \
    -H "Authorization: Bearer $TOKEN")
echo "$CLICKS" | grep -q '"total_clicks"' && pass "Click history available" || fail "Click history failed"

# Test 13: QR Code Generation (PNG)
echo "Test 13: QR Code PNG Generation"
QR_PNG=$(curl -s -I "$BASE_URL/api/urls/$SHORT_CODE/qr/png" \
    -H "Authorization: Bearer $TOKEN")
echo "$QR_PNG" | grep -q "image/png" && pass "PNG QR code generated" || fail "PNG generation failed"

# Test 14: QR Code Generation (SVG)
echo "Test 14: QR Code SVG Generation"
QR_SVG=$(curl -s "$BASE_URL/api/urls/$SHORT_CODE/qr/svg" \
    -H "Authorization: Bearer $TOKEN")
echo "$QR_SVG" | grep -q "<svg" && pass "SVG QR code generated" || fail "SVG generation failed"
echo "$QR_SVG" | grep -q "#CE422B" && pass "SVG contains Rust orange" || fail "Missing Rust branding"

# Test 15: Rename URL
echo "Test 15: Rename URL"
RENAME=$(curl -s -X PATCH "$BASE_URL/api/urls/$SHORT_CODE/name" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d '{"name":"Rust Official"}')
echo "$RENAME" | grep -q '"message"' && pass "URL renamed" || fail "Rename failed"

# Test 16: Delete URL
echo "Test 16: Delete URL"
DELETE=$(curl -s -X DELETE "$BASE_URL/api/urls/$SHORT_CODE" \
    -H "Authorization: Bearer $TOKEN")
echo "$DELETE" | grep -q '"message"' && pass "URL deleted" || fail "Delete failed"

echo ""
echo "================================="
echo -e "${GREEN}All Phase 1 tests passed!${NC}"
echo ""
echo "Note: Account lockout test requires manual testing:"
echo "  1. Try logging in with wrong password 5 times"
echo "  2. Should receive HTTP 429 with lockout message"
echo "  3. Wait 30 minutes (or adjust config) to unlock"
```

Make executable:
```bash
chmod +x test-phase1.sh
```

Run with:
```bash
./test-phase1.sh
# Or with custom URL:
BASE_URL=https://your-domain.com ./test-phase1.sh
```
```

## Expected Output
- Executable test script
- Tests all major features
- Color-coded pass/fail
- Automated curl-based tests
- Clear output for each test
- Instructions for manual tests
- Exits on first failure
