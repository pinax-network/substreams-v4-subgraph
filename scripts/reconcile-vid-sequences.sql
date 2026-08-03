\set ON_ERROR_STOP on

\if :{?target_schema}
\else
  \echo 'set target_schema to the restored Graph Node deployment schema'
  \quit
\endif

-- graphman restore --replace can retain sequence positions from the schema it
-- replaces. Bring every Graph Node entity VID sequence to the restored table's
-- actual maximum before the normal writer is allowed to resume.
SELECT format(
  'SELECT setval(%L, COALESCE(max(vid), 1), max(vid) IS NOT NULL) FROM %I.%I;',
  pg_get_serial_sequence(format('%I.%I', n.nspname, c.relname), 'vid'),
  n.nspname,
  c.relname
)
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = :'target_schema'
  AND c.relkind = 'r'
  AND EXISTS (
    SELECT 1
    FROM pg_attribute a
    WHERE a.attrelid = c.oid
      AND a.attname = 'vid'
      AND NOT a.attisdropped
  )
  AND pg_get_serial_sequence(format('%I.%I', n.nspname, c.relname), 'vid') IS NOT NULL
ORDER BY c.relname
\gexec

WITH vid_tables AS (
  SELECT
    c.relname AS table_name,
    pg_get_serial_sequence(format('%I.%I', n.nspname, c.relname), 'vid') AS sequence_name
  FROM pg_class c
  JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE n.nspname = :'target_schema'
    AND c.relkind = 'r'
    AND EXISTS (
      SELECT 1
      FROM pg_attribute a
      WHERE a.attrelid = c.oid
        AND a.attname = 'vid'
        AND NOT a.attisdropped
    )
)
SELECT format(
  'SELECT %L AS table_name, COALESCE(v.max_vid, 0) AS max_vid, s.last_value, s.is_called, '
  || '(v.max_vid IS NULL AND s.last_value = 1 AND NOT s.is_called) '
  || 'OR (v.max_vid IS NOT NULL AND s.last_value = v.max_vid AND s.is_called) AS valid '
  || 'FROM (SELECT max(vid) AS max_vid FROM %I.%I) v CROSS JOIN %s s;',
  table_name,
  :'target_schema',
  table_name,
  sequence_name
)
FROM vid_tables
WHERE sequence_name IS NOT NULL
ORDER BY table_name
\gexec
