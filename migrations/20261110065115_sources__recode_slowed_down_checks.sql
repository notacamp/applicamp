-- The production check of 2026-11-10 got only "slow down" answers from these
-- job sites, and recorded them as `no_job_list` (#117). Recode just those, so
-- the admin page says why they wait, without a re-check. On a database
-- without these rows this changes nothing.
UPDATE sources.job_boards
   SET report = jsonb_set(report, '{waiting_because}', '{"code":"asked_to_slow_down"}'::jsonb)
 WHERE status = 'candidate'
   AND report->'waiting_because'->>'code' = 'no_job_list'
   AND slug IN (
     'visionhires-com', 'solarhires-com', 'nuclearhires-com', 'semiconductorhires-com',
     'gymhires-com', 'biogasjobs-com', 'electrifyjobs-com', 'decarbonizationjobs-com',
     'carboncapturecareers-com', 'geospatialcareers-com', 'spacetechcareers-com',
     'privateclubcareers-com', 'hydrocareers-com', 'firesafetycareers-co-uk',
     'propcareers-co-uk', 'qscareers-co-uk', 'identitysecurityjobs-com',
     'industrialcyberjobs-com', 'internasia-com', 'spotjobs-com', 'robotjobsboard-com'
   );
