-- Board approvals (#26): only the owner declines a board. The seed list of
-- #5 declined Landing.jobs, The Muse, Wellfound and Dice on the owner's
-- behalf; each such row, while it still carries the seed's own reason (the
-- owner never decided on it), waits as a candidate again, with what the
-- research found as its note and, where it is a fact, as blockers (shown,
-- and they block approval). A board the owner declined is never touched.
-- Running it twice changes nothing.

UPDATE sources.job_boards
   SET status = 'candidate',
       decline_reason = NULL,
       report = coalesce(report, '{}'::jsonb) || jsonb_build_object('notes',
           'Tech jobs, Portugal and Europe. Read 2026-10-28: its terms forbid automated access and its robots.txt disallows its API.'),
       blockers = jsonb_build_array(
           jsonb_build_object(
               'code', 'terms_forbid',
               'detail', 'You may not "use, support or develop software, devices, scripts, robots or any other means or processes to access, monitor, scrape or copy the Platform".',
               'source', 'https://landing.jobs/tos'),
           jsonb_build_object(
               'code', 'robots_disallowed',
               'detail', 'robots.txt disallows /api/ and /jobs/search for every crawler.',
               'source', 'https://landing.jobs/robots.txt')),
       updated_at = now()
 WHERE slug = 'landing-jobs' AND origin = 'seed' AND status = 'declined'
   AND decline_reason = 'Its robots.txt disallows /api/ and its terms forbid scraping.';

UPDATE sources.job_boards
   SET status = 'candidate',
       decline_reason = NULL,
       report = coalesce(report, '{}'::jsonb) || jsonb_build_object('notes',
           'Mostly US jobs, and many of its remote listings are old (some from 2025).'),
       updated_at = now()
 WHERE slug = 'the-muse' AND origin = 'seed' AND status = 'declined'
   AND decline_reason = 'US-centric, and its listings are stale.';

UPDATE sources.job_boards
   SET status = 'candidate',
       decline_reason = NULL,
       report = coalesce(report, '{}'::jsonb) || jsonb_build_object('notes',
           'Startup jobs. Read 2026-10-28: no public API or feed.'),
       blockers = jsonb_build_array(
           jsonb_build_object(
               'code', 'robots_disallowed',
               'detail', 'robots.txt disallows /_jobs/, /search and links to a job by id (?jobId=, ?jobSlug=) for every crawler.',
               'source', 'https://wellfound.com/robots.txt'),
           jsonb_build_object(
               'code', 'terms_forbid',
               'detail', 'You may not "copy, disclose or distribute Content except as expressly permitted by the Terms (including through the use of automated or non-automated harvesting, collection or ''scraping'')".',
               'source', 'https://wellfound.com/terms')),
       updated_at = now()
 WHERE slug = 'wellfound' AND origin = 'seed' AND status = 'declined'
   AND decline_reason = 'Its robots.txt disallows automated reading.';

UPDATE sources.job_boards
   SET status = 'candidate',
       decline_reason = NULL,
       report = coalesce(report, '{}'::jsonb) || jsonb_build_object('notes',
           'US tech jobs. Read 2026-10-28: no public API or feed.'),
       blockers = jsonb_build_array(
           jsonb_build_object(
               'code', 'robots_disallowed',
               'detail', 'robots.txt disallows /job, /jobsearch/, /jobs?q, /rss/ and /feed/ for every crawler.',
               'source', 'https://www.dice.com/robots.txt'),
           jsonb_build_object(
               'code', 'terms_forbid',
               'detail', 'You may not "use any robot, spider, site search/retrieval application, or other manual or automatic device or process to retrieve, index, ''data mine,'' or in any way reproduce or circumvent the navigational structure or presentation of the Site or its contents".',
               'source', 'https://www.dice.com/about/terms-and-conditions')),
       updated_at = now()
 WHERE slug = 'dice' AND origin = 'seed' AND status = 'declined'
   AND decline_reason = 'Its robots.txt disallows automated reading.';
