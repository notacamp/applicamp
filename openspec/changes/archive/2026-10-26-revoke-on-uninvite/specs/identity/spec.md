## MODIFIED Requirements

### Requirement: Sign in with Campbooks or LinkedIn, invite list only
The app SHALL let a person sign in with Campbooks or with LinkedIn through
OpenID Connect (authorization code with PKCE), and SHALL create or find an
account only when the provider reports a verified email that is on the invite
list, or when the provider identity is already linked to an account whose
email is on the invite list. A provider that is not configured SHALL not be
offered.

#### Scenario: An invited person signs in for the first time
- **WHEN** a person whose verified email is on the invite list signs in with Campbooks
- **THEN** an account is created with that provider identity linked, a session starts and they land on Today

#### Scenario: Someone not on the list
- **WHEN** a person whose email is not on the invite list signs in with LinkedIn
- **THEN** no account is created, no session starts, and they see "You're not on the list yet"

#### Scenario: Both ways in reach one account
- **WHEN** an invited person who first signed in with Campbooks later signs in with LinkedIn using the same verified email
- **THEN** LinkedIn is linked to the same account and both ways in are listed on `/api/v1/me`

#### Scenario: A linked way in after the invite was removed
- **WHEN** a person whose account email was taken off the invite list signs in with a way in already linked to that account
- **THEN** no session starts, they see "You're not on the list yet", and the account, its ways in and its data are unchanged

#### Scenario: A forged callback
- **WHEN** a callback arrives with a `state` that does not match, or an ID token whose nonce, audience, issuer or signature does not verify
- **THEN** sign-in fails, no session starts and nothing about the token is logged

### Requirement: The invite list is managed from the command line
Invites SHALL be added, removed and listed only through the server's CLI;
no HTTP endpoint SHALL change the list. Removing an invite SHALL withdraw
access from the account with that email: every one of its sessions ends, and
it cannot sign in until the address is invited again. The account and its data
SHALL be kept.

#### Scenario: Adding an invite
- **WHEN** an operator runs `applicamp invite add someone@example.com`
- **THEN** that email can sign in, and `applicamp invite list` shows it

#### Scenario: Removing an invite ends the sessions
- **WHEN** an operator runs `applicamp invite remove someone@example.com` and an account has that email
- **THEN** every session of that account ends, sessions of other accounts are unchanged, and no email address is logged

#### Scenario: Inviting again restores access
- **WHEN** an operator runs `applicamp invite add someone@example.com` after removing it
- **THEN** the person signs in to the same account with the same data
