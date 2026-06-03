# ADR-0010 — Single-user; optional shared-password auth

## Status

Accepted

## Context

The application has exactly one user, the owner, on a self-hosted instance
([US-10](../requirements.md)). Multi-user accounts, roles, and sharing are out of scope. The
owner may still want to keep a self-hosted instance from being wide open
([US-19](../requirements.md)).

## Decision

Ship **no user accounts** in v1. Provide an **optional single shared password** enforced by a
`tower` middleware layer (e.g. HTTP basic auth) that can be enabled via configuration.

## Consequences

- Minimal auth surface and no user-management complexity.
- An instance can be left open (e.g. behind a private network/VPN) or password-gated, owner's choice.
- If multi-user support is ever required, this ADR is **superseded** by a new one introducing
  accounts and authorization.
