## MODIFIED Requirements

### Requirement: The person's own website is read, as assumptions to confirm
When the portrait is made and the profile holds the person's own link, Tell me about you SHALL
read that site alongside the past employers, within the same time window: its home page and up to
five pages it links to about the person, on the same host only and only where its robots.txt
allows, through the same reader and host guard, never by searching. Email addresses and phone numbers SHALL be removed from the pages before
the model reads them, and a fact naming one SHALL never be kept. One model call SHALL pick out
at most eight facts about the person's work, each with the page's exact words, and a fact SHALL be
kept only when its quote is on the page it names. Those facts SHALL reach the portrait only as
assumptions to confirm, linking their page. A site that cannot be read, or a call that fails,
SHALL leave the portrait without it, and the read SHALL count toward the daily cap.

#### Scenario: The site says what the person built
- **WHEN** the profile links to the person's site, whose projects page says "I built Ledgerly, an open-source billing toolkit"
- **THEN** the portrait holds "I assume, from your website, that you built an open-source billing toolkit", linking that page, to confirm

#### Scenario: A quote the page does not hold
- **WHEN** the model offers a fact whose quote is not on the page it names, or names a page that was not read
- **THEN** the fact never reaches the portrait

#### Scenario: The site cannot be read
- **WHEN** the site's robots.txt forbids it, or it does not answer
- **THEN** the portrait is made without it, with no extra model call

#### Scenario: The site shows how to reach the person
- **WHEN** the person's site shows their email address and phone number, and dates such as "2017 - 2021"
- **THEN** the model reads the pages with the email and phone number removed and the dates kept, and no fact names either
