\set ON_ERROR_STOP on
\pset format unaligned
\pset tuples_only on
\pset pager off

BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL statement_timeout = '5min';
SET LOCAL lock_timeout = '5s';
SET LOCAL search_path = :"schema", public;

WITH params AS (SELECT :seed_block::integer AS seed_block)
SELECT jsonb_build_object('@poi_seed', jsonb_build_object(
    'vid', vid,
    'block_range_start', lower(block_range),
    'id', id,
    'digest', digest
))::text
FROM poi2$, params
WHERE block_range @> seed_block
ORDER BY id;

WITH params AS (SELECT :seed_block::integer AS seed_block)
SELECT jsonb_build_object('@active_poi_seed_complete', jsonb_build_object(
    'seed_block', seed_block,
    'records', (SELECT count(*) FROM poi2$ WHERE block_range @> seed_block)
))::text
FROM params;

COMMIT;
