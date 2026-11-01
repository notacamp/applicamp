- **Searches no longer stall.** The model's answers were being cut off (some of its providers
  think for thousands of tokens first), so every search since 10-27 scored almost nothing and the
  nightly searches added no jobs. Calls now ask for short thinking and go only to providers that
  honour the answer's format, and a batch that still comes back unusable is scored again in two
  halves instead of ending the search. The job brief's company facts benefit too.
