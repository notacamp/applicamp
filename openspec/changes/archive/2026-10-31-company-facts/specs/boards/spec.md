## ADDED Requirements

### Requirement: A board may give the company's own website
A job board's field mapping MAY name where it gives the hiring company's
website. When a search adds a job from that board, the website SHALL be kept
on the company (never replacing one the person typed), so the company's own
pages can be read for a tailored CV without asking.

#### Scenario: The board gives the website
- **WHEN** a board lists a job with the company's website and a search adds it
- **THEN** the company's site is that website, and Prepare does not ask for it
