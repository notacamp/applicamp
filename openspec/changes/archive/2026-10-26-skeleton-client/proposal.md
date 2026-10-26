# Skeleton, client half: the app shell on @notacamp/ui

Issue: https://github.com/notacamp/applicamp/issues/3 (PR 2 of 2; the server
half is the change `skeleton-server`)

## Why
The server of `skeleton-server` signs people in; this change gives them
somewhere to land: the shell every later screen lives in, built on the
Campbooks design system (ADR 0005), and an end-to-end test of the whole
sign-in against the real stack.

## What changes
- The client shell on `@notacamp/ui`: sidebar (Today, Pipeline, Agents,
  Profile & goals), empty states, the Applicamp token set (Notion-like
  neutrals), en / pt-PT / es-ES / fr.
- The sign-in page (one button per configured provider), the "You're not on
  the list yet" page and sign-out.
- The image serves the built client; client gates and one Playwright happy
  path join CI.

## Modules touched
`identity` (client module). The server is unchanged apart from serving the
web client.

## Non-goals
- The staging deploy (notacamp/applicamp-cloud#1).
- Native apps (Tauri). Applicamp is web-only for now.
- Profile, pipeline, agents content: empty states only.
