\set ON_ERROR_STOP on
\pset format unaligned
\pset tuples_only on
\pset pager off

BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL statement_timeout = '10min';
SET LOCAL lock_timeout = '5s';
SET LOCAL search_path = :"schema", public;

WITH
params AS (
    SELECT
        :seed_block::integer AS seed_block,
        :after_vid::bigint AS after_vid,
        :page_size::integer AS page_size
),
page AS MATERIALIZED (
    SELECT
        t.vid,
        t.id,
        (to_jsonb(t) - 'block_range') ||
            jsonb_build_object('block_range_start', lower(t.block_range)) AS data
    FROM :"table" t, params
    WHERE t.block_range @> seed_block
      AND t.vid > after_vid
    ORDER BY t.vid
    LIMIT :page_size
),
output(sort_group, vid, payload) AS (
    SELECT
        0,
        vid,
        jsonb_build_object('@seed_version', jsonb_build_object(
            'entity_type', :'entity_type',
            'data', data
        ))
    FROM page

    UNION ALL

    SELECT
        1,
        0,
        jsonb_build_object('@active_seed_page_complete', jsonb_build_object(
            'entity_type', :'entity_type',
            'seed_block', (SELECT seed_block FROM params),
            'after_vid', (SELECT after_vid FROM params),
            'records', (SELECT count(*) FROM page),
            'last_vid', coalesce((SELECT max(vid) FROM page), (SELECT after_vid FROM params))
        ))
)
SELECT payload::text FROM output ORDER BY sort_group, vid;

COMMIT;
