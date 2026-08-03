\set ON_ERROR_STOP on
\pset format unaligned
\pset tuples_only on
\pset pager off

BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL statement_timeout = '30min';
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

WITH
params AS (SELECT :seed_block::integer AS seed_block),
records(entity_type, id, data) AS (
    SELECT 'PoolManager', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM pool_manager t, params WHERE t.block_range @> seed_block
    UNION ALL
    SELECT 'Bundle', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM bundle t, params WHERE t.block_range @> seed_block
    UNION ALL
    SELECT 'Token', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM token t, params WHERE t.block_range @> seed_block
    UNION ALL
    SELECT 'Pool', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM pool t, params WHERE t.block_range @> seed_block
    UNION ALL
    SELECT 'Tick', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM tick t, params WHERE t.block_range @> seed_block
    UNION ALL
    SELECT 'UniswapDayData', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM uniswap_day_data t, params WHERE t.block_range @> seed_block
    UNION ALL
    SELECT 'PoolDayData', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM pool_day_data t, params WHERE t.block_range @> seed_block
    UNION ALL
    SELECT 'PoolHourData', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM pool_hour_data t, params WHERE t.block_range @> seed_block
    UNION ALL
    SELECT 'TokenDayData', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM token_day_data t, params WHERE t.block_range @> seed_block
    UNION ALL
    SELECT 'TokenHourData', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM token_hour_data t, params WHERE t.block_range @> seed_block
    UNION ALL
    SELECT 'Position', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM position t, params WHERE t.block_range @> seed_block
    UNION ALL
    SELECT 'ArrakisHook', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM arrakis_hook t, params WHERE t.block_range @> seed_block
),
output(sort_group, entity_type, id, payload) AS (
    SELECT
        0,
        entity_type,
        id,
        jsonb_build_object(
            '@seed_version', jsonb_build_object('entity_type', entity_type, 'data', data)
        )
    FROM records

    UNION ALL

    SELECT
        1,
        '',
        '',
        jsonb_build_object('@active_seed_complete', jsonb_build_object(
            'seed_block', (SELECT seed_block FROM params),
            'seed_records', (SELECT count(*) FROM records),
            'poi_records', (
                SELECT count(*) FROM poi2$, params WHERE block_range @> seed_block
            )
        ))
)
SELECT payload::text FROM output ORDER BY sort_group, entity_type, id;

COMMIT;
