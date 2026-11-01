## ADDED Requirements

### Requirement: One bad model answer never ends a search
A search SHALL score a batch whose model answer is unusable (cut off, or not the JSON asked for)
again as two halves, and SHALL leave the jobs of a half that fails again for the next search.
Only four failed scoring calls in a row SHALL end a search's scoring (`ai_failed`), keeping what
was scored. On OpenRouter, scoring calls SHALL ask for low reasoning effort and SHALL be routed
only to providers that support every parameter sent.

#### Scenario: An answer is cut off
- **WHEN** the answer for a batch of ten is cut off and the answers for five jobs are not
- **THEN** the ten are scored as two batches of five, the search goes on, and it ends without `ai_failed`

#### Scenario: The provider keeps failing
- **WHEN** every scoring answer is unusable
- **THEN** the search makes four calls, then ends its scoring with `ai_failed`
