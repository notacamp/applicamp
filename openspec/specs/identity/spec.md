# identity Specification

## Purpose
Who may use Applicamp and how they sign in: one account per person, reached
through Campbooks or LinkedIn (OpenID Connect), created only for addresses on
an invite list managed from the server's command line, with the session held
by the server.

## Requirements

### Requirement: Sign in with Campbooks or LinkedIn, invite list only
The app SHALL let a person sign in with Campbooks or with LinkedIn through
OpenID Connect (authorization code with PKCE), and SHALL create or find an
account only when the provider reports a verified email that is on the invite
list, or when the provider identity is already linked to an account. A
provider that is not configured SHALL not be offered.

#### Scenario: An invited person signs in for the first time
- **WHEN** a person whose verified email is on the invite list signs in with Campbooks
- **THEN** an account is created with that provider identity linked, a session starts and they land on Today

#### Scenario: Someone not on the list
- **WHEN** a person whose email is not on the invite list signs in with LinkedIn
- **THEN** no account is created, no session starts, and they see "You're not on the list yet"

#### Scenario: Both ways in reach one account
- **WHEN** an invited person who first signed in with Campbooks later signs in with LinkedIn using the same verified email
- **THEN** LinkedIn is linked to the same account and both ways in are listed on `/api/v1/me`

#### Scenario: A forged callback
- **WHEN** a callback arrives with a `state` that does not match, or an ID token whose nonce, audience, issuer or signature does not verify
- **THEN** sign-in fails, no session starts and nothing about the token is logged

### Requirement: Sessions stay on the server
The browser SHALL hold only an httpOnly, Secure, SameSite=Strict session
cookie; provider tokens SHALL never reach browser JavaScript or logs, and
stored refresh tokens SHALL be encrypted. Every state-changing request SHALL
carry the CSRF header from the app's own origin.

#### Scenario: Sign out
- **WHEN** a signed-in person signs out
- **THEN** the session is deleted on the server, the cookie is cleared and they land on the sign-in page

#### Scenario: A cross-site request
- **WHEN** a POST arrives without `X-Applicamp-CSRF: 1` or from another origin
- **THEN** it is refused and nothing changes

### Requirement: The invite list is managed from the command line
Invites SHALL be added, removed and listed only through the server's CLI;
no HTTP endpoint SHALL change the list.

#### Scenario: Adding an invite
- **WHEN** an operator runs `applicamp invite add someone@example.com`
- **THEN** that email can sign in, and `applicamp invite list` shows it
