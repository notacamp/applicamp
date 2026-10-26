## ADDED Requirements

### Requirement: The app shell
A signed-in person SHALL see a sidebar with Today, Pipeline, Agents and
Profile & goals, each with a designed empty state that says what will appear
there, in their language (English, Portuguese, Spanish or French), at phone
and desktop widths, in light and dark themes.

#### Scenario: First visit after sign-in
- **WHEN** a person signs in for the first time
- **THEN** Today shows that nothing needs them yet and offers to set up their profile

#### Scenario: Someone not on the list
- **WHEN** the server sends a browser to `/not-invited`
- **THEN** it shows "You're not on the list yet" and a way back to the sign-in page
