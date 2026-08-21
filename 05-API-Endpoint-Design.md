# Universal Media Downloader — API Endpoint Design

## 1. API Strategy

The desktop app does not need a backend for core downloading.

The optional cloud API handles:

- Authentication.
- User profile.
- Device registration.
- Licensing.
- Subscription state.
- Entitlements.
- Remote configuration.
- Optional telemetry.

Example base URL:

`https://api.example.com/v1`

Authentication:

`Authorization: Bearer <access_token>`

## 2. Authentication

### POST /auth/register

```json
{
  "email": "user@example.com",
  "password": "..."
}
```

### POST /auth/login

```json
{
  "email": "user@example.com",
  "password": "..."
}
```

### POST /auth/refresh

```json
{
  "refresh_token": "..."
}
```

### POST /auth/logout

Invalidates the refresh-token session.

## 3. User

### GET /me

Returns current user.

### PATCH /me

Updates allowed profile fields.

## 4. Devices

### GET /devices

Returns registered devices.

### POST /devices

```json
{
  "device_name": "Malik PC",
  "platform": "windows",
  "app_version": "1.0.0"
}
```

### DELETE /devices/{deviceId}

Deactivates a device.

## 5. Licensing

### GET /license

Example:

```json
{
  "plan": "pro",
  "status": "active",
  "expires_at": "2027-01-01T00:00:00Z",
  "features": [
    "unlimited_queue",
    "scheduler",
    "advanced_naming"
  ]
}
```

### POST /license/activate

```json
{
  "license_key": "XXXX-XXXX-XXXX",
  "device_id": "dev_123"
}
```

### POST /license/validate

Validates installed license/device state.

### POST /license/deactivate

Releases device activation.

## 6. Remote Configuration

### GET /config

```json
{
  "minimum_supported_version": "1.0.0",
  "latest_version": "1.2.0",
  "features": {
    "scheduler": true,
    "cloud_sync": false
  }
}
```

Security-sensitive features must fail closed if configuration cannot be validated.

## 7. Telemetry

Telemetry is opt-in.

### POST /telemetry/events

```json
{
  "events": [
    {
      "name": "download_completed",
      "timestamp": "2026-08-20T10:00:00Z",
      "properties": {
        "platform": "youtube",
        "duration_ms": 123456
      }
    }
  ]
}
```

Never send downloaded media, private URLs, cookies, passwords, access tokens, or personal filesystem paths.

## 8. Health

### GET /health

Load balancer/service health.

### GET /version

Returns API version.

## 9. Error Format

```json
{
  "error": {
    "code": "DEVICE_LIMIT_REACHED",
    "message": "Maximum device activations reached.",
    "request_id": "req_123",
    "retryable": false
  }
}
```

## 10. HTTP Status Codes

- `200` Success.
- `201` Created.
- `204` No content.
- `400` Validation error.
- `401` Authentication required.
- `403` Forbidden/entitlement denied.
- `404` Not found.
- `409` Conflict.
- `422` Semantic validation error.
- `429` Rate limited.
- `500` Internal server error.
- `503` Temporarily unavailable.

## 11. Idempotency

Use:

`Idempotency-Key: <unique-key>`

for retryable mutating operations such as device registration, license activation, and telemetry batches.

## 12. Example Rate Limits

Authentication: 10 req/min/IP.

License: 30 req/min/device.

General API: 120 req/min/user.

Telemetry: 300 req/min/device.

These are configurable server-side values.

## 13. Versioning

Use:

`/v1/...`

Breaking changes use `/v2/...`.

## 14. Cloud Database

### users

```sql
CREATE TABLE users (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
```

### devices

```sql
CREATE TABLE devices (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    device_name TEXT NOT NULL,
    platform TEXT NOT NULL,
    app_version TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    last_seen_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_devices_user_id ON devices(user_id);
```

### subscriptions

```sql
CREATE TABLE subscriptions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    provider TEXT NOT NULL,
    provider_subscription_id TEXT,
    plan TEXT NOT NULL,
    status TEXT NOT NULL,
    current_period_end TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
```

### license_activations

```sql
CREATE TABLE license_activations (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    device_id UUID NOT NULL REFERENCES devices(id),
    plan TEXT NOT NULL,
    status TEXT NOT NULL,
    activated_at TIMESTAMPTZ NOT NULL,
    deactivated_at TIMESTAMPTZ
);

CREATE INDEX idx_license_activations_user
ON license_activations(user_id);
```

## 15. API Security

- TLS only.
- Secure headers.
- Short-lived access tokens.
- Rotating refresh tokens.
- Argon2id password hashing.
- Session/device revocation.
- Server-side entitlement checks.
- Rate limiting.
- Audit logs for license actions.
- No media proxying through the API.

## 16. API Documentation

Use OpenAPI 3.1 and maintain:

`docs/openapi.yaml`

Generate Swagger UI, TypeScript types, and contract tests from the specification.
