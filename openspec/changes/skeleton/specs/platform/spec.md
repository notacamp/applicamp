## ADDED Requirements

### Requirement: Every endpoint is in the OpenAPI document
Every HTTP endpoint SHALL be described in the generated OpenAPI document
with an explicit security requirement, and the client SHALL use only types
generated from it.

#### Scenario: An undocumented endpoint
- **WHEN** a route exists that the OpenAPI document does not describe, or the committed document is stale
- **THEN** CI fails

### Requirement: The app shell
A signed-in person SHALL see a sidebar with Today, Pipeline, Agents and
Profile & goals, each with a designed empty state that says what will appear
there, in their language (English, Portuguese, Spanish or French), at phone
and desktop widths, in light and dark themes.

#### Scenario: First visit after sign-in
- **WHEN** a person signs in for the first time
- **THEN** Today shows that nothing needs them yet and offers to set up their profile
