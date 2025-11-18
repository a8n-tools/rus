# Chunk 2: Password Complexity Validation

## Context
Building on Chunk 1's configuration system. We need to enforce strong password requirements for security hardening.

## Goal
Add password validation that enforces: 8+ characters, at least one uppercase, one number, one special character.

## Prompt

```text
I have a Rust URL shortener with a Config system. Now I need to add password complexity validation.

Current register endpoint only checks:
- Username/password not empty
- Username minimum 3 chars
- Password minimum 6 chars (too weak)

Create a validate_password() function that:
1. Takes password as &str
2. Returns Result<(), String> where Err contains descriptive message
3. Checks:
   - Length >= 8 characters
   - Contains at least one uppercase letter (use char.is_uppercase())
   - Contains at least one number (use char.is_numeric())
   - Contains at least one special character (use !char.is_alphanumeric())
4. Return specific error message for each failed check:
   - "Password must be at least 8 characters long"
   - "Password must contain at least one uppercase letter"
   - "Password must contain at least one number"
   - "Password must contain at least one special character"

Integrate into the register() handler:
1. After checking username length
2. Call validate_password(&req.password)
3. If Err, return BadRequest with the error message in JSON format: {"error": "message"}
4. Remove the old "password at least 6 characters" check

The validation should happen BEFORE hashing the password to fail fast.

Example valid passwords:
- "MyP@ssw0rd" ✓
- "Secure123!" ✓

Example invalid passwords:
- "password" ✗ (no uppercase, no number, no special)
- "Password1" ✗ (no special character)
- "Pass@1" ✗ (too short)
```

## Expected Output
- validate_password() function
- Descriptive error messages
- Integrated into register endpoint
- Old weak validation removed
