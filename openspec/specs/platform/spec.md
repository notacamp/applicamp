# platform Specification

## Purpose
What every part of Applicamp shares: the HTTP API described by one generated
OpenAPI document that clients build on, and the app shell people work in.

## Requirements

### Requirement: Every endpoint is in the OpenAPI document
Every HTTP endpoint SHALL be described in the generated OpenAPI document
with an explicit security requirement, and the client SHALL use only types
generated from it.

#### Scenario: An undocumented endpoint
- **WHEN** a route exists that the OpenAPI document does not describe, or the committed document is stale
- **THEN** CI fails

#### Scenario: An operation without a security requirement
- **WHEN** an operation in the document declares neither `security(())` nor the session scheme
- **THEN** CI fails
