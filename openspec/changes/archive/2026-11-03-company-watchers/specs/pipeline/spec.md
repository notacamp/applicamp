## ADDED Requirements

### Requirement: Today says when a watched company posted a job that fits
Today SHALL show, for each job found in the last 3 days on the own job board of a company the person watches that is still Found, not set
aside and not yet opened, a card "X posted a job that fits you" with the role, place and pay, when it was found on their own job board, how
well it fits and that it is in the Pipeline as Found. Open it SHALL open that job's peek in the Pipeline; Not for me SHALL set it aside. The
opportunity SHALL credit the company's own job board among the boards it was found on. Today SHALL also offer a compact suggestion of one
company to watch, with Watch, Not interested and Why?, unless the person has not set up a profile.

#### Scenario: A job from a watched board
- **WHEN** a search adds a job that fits from a watched company's board
- **THEN** Today shows "Northwind Stays posted a job that fits you" with Open it and Not for me, and the job's peek says it was found on Northwind Stays' job board

#### Scenario: Not for me
- **WHEN** the person presses Not for me on that card
- **THEN** the job is set aside like any other and the card goes
