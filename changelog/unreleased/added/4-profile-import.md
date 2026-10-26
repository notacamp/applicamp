- **Import your profile.** On Profile & goals (and from Today's "Set up your profile"), upload
  your LinkedIn data export ZIP, a PDF (your CV, or your LinkedIn profile) or paste text. You
  get a draft to review, marked new, changed or the same against your profile, then Replace,
  Merge or Discard; nothing is saved before that and the file is not kept. The ZIP is read on
  the server without AI; a PDF or text is read by the `complete` model
  (`APPLICAMP_AI__COMPLETE__BASE_URL`, `__MODEL`, `__API_KEY_FILE`; a per-account daily token
  cap, `APPLICAMP_AI__DAILY_TOKENS`), and any detail it gives that the file does not say is
  left out and counted. The profile then reads like Notion properties and lists, each value
  edited in place.
