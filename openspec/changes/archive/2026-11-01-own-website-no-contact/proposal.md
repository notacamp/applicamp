# The person's own website reaches the model without their contact details

Issue: https://github.com/notacamp/applicamp/issues/64

## Why
Checking the owner's site from the hosted instance after #61: personal sites usually show an
email address and a phone number, and the site call sent the pages' text to the model as it was.
The portrait's own call never sees contact details; the site call should not either.

## What changes
- Email addresses and phone numbers (nine or more digits with the usual separators; a date range
  such as "2017 - 2021" has eight and stays) are replaced by `[email]` and `[phone]` before the
  model reads the pages, and quotes are checked against the same text.
- A fact whose claim or quote names an email address or a phone number is never kept.
