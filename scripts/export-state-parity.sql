\set ON_ERROR_STOP on
\pset format unaligned
\pset tuples_only on
\pset pager off

BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL statement_timeout = '5min';
SET LOCAL lock_timeout = '5s';
SET LOCAL search_path = :"schema", public;

\if :include_table_max
-- The differential fixture retains only active mutable seed rows. Its VID
-- allocation domain therefore starts at those rows' maxima; omitted immutable
-- history starts at -1. Production retained-history generation instead takes
-- exact per-table maxima from the native graft dump in capture-active-seed.sh.
WITH params AS (SELECT :seed_block::integer AS seed_block), table_max(entity_type, max_vid) AS (
    SELECT 'PoolManager', coalesce(max(vid), -1) FROM pool_manager, params WHERE block_range @> seed_block
    UNION ALL SELECT 'Bundle', coalesce(max(vid), -1) FROM bundle, params WHERE block_range @> seed_block
    UNION ALL SELECT 'Token', coalesce(max(vid), -1) FROM token, params WHERE block_range @> seed_block
    UNION ALL SELECT 'Pool', coalesce(max(vid), -1) FROM pool, params WHERE block_range @> seed_block
    UNION ALL SELECT 'Tick', coalesce(max(vid), -1) FROM tick, params WHERE block_range @> seed_block
    UNION ALL SELECT 'UniswapDayData', coalesce(max(vid), -1) FROM uniswap_day_data, params WHERE block_range @> seed_block
    UNION ALL SELECT 'PoolDayData', coalesce(max(vid), -1) FROM pool_day_data, params WHERE block_range @> seed_block
    UNION ALL SELECT 'PoolHourData', coalesce(max(vid), -1) FROM pool_hour_data, params WHERE block_range @> seed_block
    UNION ALL SELECT 'TokenDayData', coalesce(max(vid), -1) FROM token_day_data, params WHERE block_range @> seed_block
    UNION ALL SELECT 'TokenHourData', coalesce(max(vid), -1) FROM token_hour_data, params WHERE block_range @> seed_block
    UNION ALL SELECT 'Position', coalesce(max(vid), -1) FROM position, params WHERE block_range @> seed_block
    UNION ALL SELECT 'ArrakisHook', coalesce(max(vid), -1) FROM arrakis_hook, params WHERE block_range @> seed_block
    UNION ALL SELECT 'Transaction', -1
    UNION ALL SELECT 'Swap', -1
    UNION ALL SELECT 'ModifyLiquidity', -1
    UNION ALL SELECT 'Subscribe', -1
    UNION ALL SELECT 'Unsubscribe', -1
    UNION ALL SELECT 'Transfer', -1
    UNION ALL SELECT 'Poi$', coalesce(max(vid), -1) FROM poi2$, params WHERE block_range @> seed_block
)
SELECT jsonb_build_object(
    '@table', jsonb_build_object('entity_type', entity_type, 'max_vid', max_vid)
)::text
FROM table_max ORDER BY entity_type;
\endif

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

WITH params AS (SELECT :end_block::integer AS end_block)
SELECT jsonb_build_object('@poi_expected', jsonb_build_object(
    'id', id,
    'digest', digest
))::text
FROM poi2$, params
WHERE block_range @> end_block
ORDER BY id;

WITH
params AS (
    SELECT
        :seed_block::integer AS seed_block,
        :start_block::integer AS start_block,
        :end_block::integer AS end_block
),
changed_pool_manager_ids AS (
    SELECT DISTINCT id FROM pool_manager, params
    WHERE lower(block_range) BETWEEN start_block AND end_block
),
changed_bundle_ids AS (
    SELECT DISTINCT id FROM bundle, params
    WHERE lower(block_range) BETWEEN start_block AND end_block
),
changed_token_ids AS (
    SELECT DISTINCT id FROM token, params
    WHERE lower(block_range) BETWEEN start_block AND end_block
),
changed_pool_ids AS (
    SELECT DISTINCT id FROM pool, params
    WHERE lower(block_range) BETWEEN start_block AND end_block
),
changed_tick_ids AS (
    SELECT DISTINCT id FROM tick, params
    WHERE lower(block_range) BETWEEN start_block AND end_block
),
changed_uniswap_day_ids AS (
    SELECT DISTINCT id FROM uniswap_day_data, params
    WHERE lower(block_range) BETWEEN start_block AND end_block
),
changed_pool_day_ids AS (
    SELECT DISTINCT id FROM pool_day_data, params
    WHERE lower(block_range) BETWEEN start_block AND end_block
),
changed_pool_hour_ids AS (
    SELECT DISTINCT id FROM pool_hour_data, params
    WHERE lower(block_range) BETWEEN start_block AND end_block
),
changed_token_day_ids AS (
    SELECT DISTINCT id FROM token_day_data, params
    WHERE lower(block_range) BETWEEN start_block AND end_block
),
changed_token_hour_ids AS (
    SELECT DISTINCT id FROM token_hour_data, params
    WHERE lower(block_range) BETWEEN start_block AND end_block
),
changed_position_ids AS (
    SELECT DISTINCT id FROM position, params
    WHERE lower(block_range) BETWEEN start_block AND end_block
),
changed_arrakis_hook_ids AS (
    SELECT DISTINCT id FROM arrakis_hook, params
    WHERE lower(block_range) BETWEEN start_block AND end_block
),
touched_pool_ids AS (
    SELECT id FROM changed_pool_ids
    UNION
    SELECT pool FROM swap, params WHERE block$ BETWEEN start_block AND end_block
    UNION
    SELECT pool FROM modify_liquidity, params WHERE block$ BETWEEN start_block AND end_block
),
touched_pool_rows AS (
    SELECT pool.*
    FROM pool, params
    WHERE block_range @> end_block
      AND id IN (SELECT id FROM touched_pool_ids)
),
touched_token_ids AS (
    SELECT id FROM changed_token_ids
    UNION
    SELECT token_0 FROM touched_pool_rows
    UNION
    SELECT token_1 FROM touched_pool_rows
),
touched_tick_ids AS (
    SELECT id FROM changed_tick_ids
    UNION
    SELECT pool || '#' || tick_lower::text
    FROM modify_liquidity, params WHERE block$ BETWEEN start_block AND end_block
    UNION
    SELECT pool || '#' || tick_upper::text
    FROM modify_liquidity, params WHERE block$ BETWEEN start_block AND end_block
),
mapped_event_pool_times AS (
    SELECT pool, timestamp FROM swap, params WHERE block$ BETWEEN start_block AND end_block
    UNION ALL
    SELECT pool, timestamp FROM modify_liquidity, params WHERE block$ BETWEEN start_block AND end_block
),
pool_event_times AS (
    SELECT pool, timestamp FROM mapped_event_pool_times
    UNION ALL
    SELECT id, created_at_timestamp
    FROM touched_pool_rows, params
    WHERE created_at_block_number BETWEEN start_block AND end_block
),
token_event_times AS (
    SELECT pool.token_0 AS token, event.timestamp
    FROM pool_event_times event JOIN touched_pool_rows pool ON pool.id = event.pool
    UNION ALL
    SELECT pool.token_1 AS token, event.timestamp
    FROM pool_event_times event JOIN touched_pool_rows pool ON pool.id = event.pool
),
touched_uniswap_day_ids AS (
    SELECT id FROM changed_uniswap_day_ids
    UNION
    SELECT floor(timestamp::numeric / 86400)::bigint::text FROM mapped_event_pool_times
),
touched_pool_day_ids AS (
    SELECT id FROM changed_pool_day_ids
    UNION
    SELECT pool || '-' || floor(timestamp::numeric / 86400)::bigint::text FROM pool_event_times
),
touched_pool_hour_ids AS (
    SELECT id FROM changed_pool_hour_ids
    UNION
    SELECT pool || '-' || floor(timestamp::numeric / 3600)::bigint::text FROM pool_event_times
),
touched_token_day_ids AS (
    SELECT id FROM changed_token_day_ids
    UNION
    SELECT token || '-' || floor(timestamp::numeric / 86400)::bigint::text FROM token_event_times
),
touched_token_hour_ids AS (
    SELECT id FROM changed_token_hour_ids
    UNION
    SELECT token || '-' || floor(timestamp::numeric / 3600)::bigint::text FROM token_event_times
),
touched_position_ids AS (
    SELECT id FROM changed_position_ids
    UNION
    SELECT token_id::text FROM transfer, params WHERE block$ BETWEEN start_block AND end_block
),
changed_token_seed_rows AS (
    SELECT token.*
    FROM token, params
    WHERE block_range @> seed_block
      AND id IN (SELECT id FROM touched_token_ids)
),
pricing_pool_ids AS (
    SELECT DISTINCT unnest(whitelist_pools) AS id
    FROM changed_token_seed_rows
),
seed_pool_ids AS (
    SELECT id FROM touched_pool_ids
    UNION
    SELECT id FROM pricing_pool_ids
    UNION
    SELECT '0x90333bb05c258fe0dddb2840ef66f1a05165aa7dac6815d24e807cc6ebd943a0'
),
seed_pool_rows AS (
    SELECT pool.*
    FROM pool, params
    WHERE block_range @> seed_block
      AND id IN (SELECT id FROM seed_pool_ids)
),
seed_token_ids AS (
    SELECT id FROM touched_token_ids
    UNION
    SELECT token_0 FROM seed_pool_rows
    UNION
    SELECT token_1 FROM seed_pool_rows
),
records(phase, entity_type, id, data) AS (
    SELECT 0, 'PoolManager', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM pool_manager t, params
    WHERE t.block_range @> seed_block
      AND t.id = '0x498581ff718922c3f8e6a244956af099b2652b2b'
    UNION ALL
    SELECT 0, 'Bundle', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM bundle t, params WHERE t.block_range @> seed_block AND t.id = '1'
    UNION ALL
    SELECT 0, 'Token', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM token t, params
    WHERE t.block_range @> seed_block AND t.id IN (SELECT id FROM seed_token_ids)
    UNION ALL
    SELECT 0, 'Pool', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM pool t, params
    WHERE t.block_range @> seed_block AND t.id IN (SELECT id FROM seed_pool_ids)
    UNION ALL
    SELECT 0, 'Tick', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM tick t, params
    WHERE t.block_range @> seed_block AND t.id IN (SELECT id FROM touched_tick_ids)
    UNION ALL
    SELECT 0, 'UniswapDayData', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM uniswap_day_data t, params
    WHERE t.block_range @> seed_block AND t.id IN (SELECT id FROM touched_uniswap_day_ids)
    UNION ALL
    SELECT 0, 'PoolDayData', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM pool_day_data t, params
    WHERE t.block_range @> seed_block AND t.id IN (SELECT id FROM touched_pool_day_ids)
    UNION ALL
    SELECT 0, 'PoolHourData', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM pool_hour_data t, params
    WHERE t.block_range @> seed_block AND t.id IN (SELECT id FROM touched_pool_hour_ids)
    UNION ALL
    SELECT 0, 'TokenDayData', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM token_day_data t, params
    WHERE t.block_range @> seed_block AND t.id IN (SELECT id FROM touched_token_day_ids)
    UNION ALL
    SELECT 0, 'TokenHourData', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM token_hour_data t, params
    WHERE t.block_range @> seed_block AND t.id IN (SELECT id FROM touched_token_hour_ids)
    UNION ALL
    SELECT 0, 'Position', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM position t, params
    WHERE t.block_range @> seed_block AND t.id IN (SELECT id FROM touched_position_ids)
    UNION ALL
    SELECT 0, 'ArrakisHook', t.id, (to_jsonb(t) - 'block_range') || jsonb_build_object('block_range_start', lower(t.block_range))
    FROM arrakis_hook t, params
    WHERE t.block_range @> seed_block AND t.id IN (SELECT id FROM changed_arrakis_hook_ids)

    UNION ALL

    SELECT 1, 'PoolManager', t.id, to_jsonb(t) - ARRAY['vid', 'block_range']
    FROM pool_manager t, params
    WHERE t.block_range @> end_block
      AND t.id = '0x498581ff718922c3f8e6a244956af099b2652b2b'
    UNION ALL
    SELECT 1, 'Bundle', t.id, to_jsonb(t) - ARRAY['vid', 'block_range']
    FROM bundle t, params
    WHERE t.block_range @> end_block AND t.id = '1'
    UNION ALL
    SELECT 1, 'Token', t.id, to_jsonb(t) - ARRAY['vid', 'block_range']
    FROM token t, params
    WHERE t.block_range @> end_block AND t.id IN (SELECT id FROM touched_token_ids)
    UNION ALL
    SELECT 1, 'Pool', t.id, to_jsonb(t) - ARRAY['vid', 'block_range']
    FROM pool t, params
    WHERE t.block_range @> end_block AND t.id IN (SELECT id FROM touched_pool_ids)
    UNION ALL
    SELECT 1, 'Tick', t.id, to_jsonb(t) - ARRAY['vid', 'block_range']
    FROM tick t, params
    WHERE t.block_range @> end_block AND t.id IN (SELECT id FROM touched_tick_ids)
    UNION ALL
    SELECT 1, 'UniswapDayData', t.id, to_jsonb(t) - ARRAY['vid', 'block_range']
    FROM uniswap_day_data t, params
    WHERE t.block_range @> end_block AND t.id IN (SELECT id FROM touched_uniswap_day_ids)
    UNION ALL
    SELECT 1, 'PoolDayData', t.id, to_jsonb(t) - ARRAY['vid', 'block_range']
    FROM pool_day_data t, params
    WHERE t.block_range @> end_block AND t.id IN (SELECT id FROM touched_pool_day_ids)
    UNION ALL
    SELECT 1, 'PoolHourData', t.id, to_jsonb(t) - ARRAY['vid', 'block_range']
    FROM pool_hour_data t, params
    WHERE t.block_range @> end_block AND t.id IN (SELECT id FROM touched_pool_hour_ids)
    UNION ALL
    SELECT 1, 'TokenDayData', t.id, to_jsonb(t) - ARRAY['vid', 'block_range']
    FROM token_day_data t, params
    WHERE t.block_range @> end_block AND t.id IN (SELECT id FROM touched_token_day_ids)
    UNION ALL
    SELECT 1, 'TokenHourData', t.id, to_jsonb(t) - ARRAY['vid', 'block_range']
    FROM token_hour_data t, params
    WHERE t.block_range @> end_block AND t.id IN (SELECT id FROM touched_token_hour_ids)
    UNION ALL
    SELECT 1, 'Position', t.id, to_jsonb(t) - ARRAY['vid', 'block_range']
    FROM position t, params
    WHERE t.block_range @> end_block AND t.id IN (SELECT id FROM touched_position_ids)
    UNION ALL
    SELECT 1, 'ArrakisHook', t.id, to_jsonb(t) - ARRAY['vid', 'block_range']
    FROM arrakis_hook t, params
    WHERE t.block_range @> end_block AND t.id IN (SELECT id FROM changed_arrakis_hook_ids)
    UNION ALL
    SELECT 1, 'Transaction', t.id, to_jsonb(t) - ARRAY['vid', 'block$']
    FROM transaction t, params WHERE t.block$ BETWEEN start_block AND end_block
    UNION ALL
    SELECT 1, 'Swap', t.id, to_jsonb(t) - ARRAY['vid', 'block$']
    FROM swap t, params WHERE t.block$ BETWEEN start_block AND end_block
    UNION ALL
    SELECT 1, 'ModifyLiquidity', t.id, to_jsonb(t) - ARRAY['vid', 'block$']
    FROM modify_liquidity t, params WHERE t.block$ BETWEEN start_block AND end_block
    UNION ALL
    SELECT 1, 'Subscribe', t.id, to_jsonb(t) - ARRAY['vid', 'block$']
    FROM subscribe t, params WHERE t.block$ BETWEEN start_block AND end_block
    UNION ALL
    SELECT 1, 'Unsubscribe', t.id, to_jsonb(t) - ARRAY['vid', 'block$']
    FROM unsubscribe t, params WHERE t.block$ BETWEEN start_block AND end_block
    UNION ALL
    SELECT 1, 'Transfer', t.id, to_jsonb(t) - ARRAY['vid', 'block$']
    FROM transfer t, params WHERE t.block$ BETWEEN start_block AND end_block
)
SELECT payload::text
FROM (
    SELECT
        phase,
        entity_type,
        id,
        CASE phase
            WHEN 0 THEN jsonb_build_object(
                '@seed_version', jsonb_build_object('entity_type', entity_type, 'data', data)
            )
            ELSE jsonb_build_object(
                '@expected', jsonb_build_object('entity_type', entity_type, 'data', data)
            )
        END AS payload
    FROM records

    UNION ALL

    SELECT
        2,
        '',
        '',
        jsonb_build_object('@export_complete', jsonb_build_object(
            'table_records', CASE WHEN :include_table_max::integer = 1 THEN 19 ELSE 0 END,
            'poi_seed_records', (
                SELECT count(*) FROM poi2$, params WHERE block_range @> seed_block
            ),
            'poi_expected_records', (
                SELECT count(*) FROM poi2$, params WHERE block_range @> end_block
            ),
            'seed_records', (SELECT count(*) FROM records WHERE phase = 0),
            'expected_records', (SELECT count(*) FROM records WHERE phase = 1)
        ))
) output
ORDER BY phase, entity_type, id;

COMMIT;
