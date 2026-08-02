#!/usr/bin/env node

import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { readFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const fixtures = JSON.parse(await readFile(resolve(root, 'fixtures/base-ranges.json'), 'utf8'))
const lock = JSON.parse(
  await readFile(
    resolve(
      root,
      'artifacts/deployment/Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB/artifacts.lock.json',
    ),
    'utf8',
  ),
)
const endpoint = process.env.ENDPOINT || 'base-substreams-tier1-prod.kan-sst2.pinax.io:443'
const sourceAddresses = new Map(
  lock.data_sources.map(({ name, address }) => [`DATA_SOURCE_${snake(name)}`, address.toLowerCase()]),
)
const payloads = new Map([
  ['initialize', 'Initialize'],
  ['modifyLiquidity', 'ModifyLiquidity'],
  ['swap', 'Swap'],
  ['subscription', 'Subscription'],
  ['unsubscription', 'Unsubscription'],
  ['transfer', 'Transfer'],
  ['logCreatePrivateHook', 'LogCreatePrivateHook'],
])

for (const fixture of fixtures.ranges) {
  const result = spawnSync(
    'substreams',
    [
      'run',
      '-e',
      endpoint,
      'substreams.yaml',
      'map_events',
      '-s',
      String(fixture.start_block),
      '-t',
      String(fixture.end_block + 1),
      '-o',
      'jsonl',
      '--final-blocks-only',
    ],
    { cwd: root, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 },
  )
  assert.equal(
    result.status,
    0,
    `${fixture.name}: Substreams failed\n${result.stdout}\n${result.stderr}`,
  )

  const blocks = result.stdout
    .split('\n')
    .filter((line) => line.startsWith('{"@module"'))
    .map((line) => JSON.parse(line))
  const events = blocks.flatMap((block) => block['@data']?.events || [])
  const counts = Object.fromEntries(
    Object.keys(fixture.expected_events).map((event) => [event, 0]),
  )

  for (const block of blocks) {
    const blockEvents = block['@data']?.events || []
    let previousOrder = -1
    for (const event of blockEvents) {
      const presentPayloads = [...payloads.keys()].filter((key) => event[key] !== undefined)
      assert.equal(presentPayloads.length, 1, `${fixture.name}: one event payload`)
      const eventName = payloads.get(presentPayloads[0])
      assert.ok(eventName in counts, `${fixture.name}: unexpected ${eventName}`)
      counts[eventName] += 1

      const triggerOrder = Number(event.log.graphNodeTriggerOrder || 0)
      assert.equal(
        triggerOrder,
        event.log.blockLogIndex || 0,
        `${fixture.name}: Graph Node trigger order`,
      )
      assert.ok(
        triggerOrder > previousOrder,
        `${fixture.name}: event order in block ${block['@block']}`,
      )
      previousOrder = triggerOrder
      assert.equal(
        event.log.address.toLowerCase(),
        sourceAddresses.get(event.source),
        `${fixture.name}: ${event.source} address`,
      )
      assert.match(event.transaction.origin, /^0x[0-9a-f]{40}$/i)
    }
  }

  assert.deepEqual(counts, fixture.expected_events, `${fixture.name}: event counts`)
  assert.equal(
    events.find((event) => Number(event.block.number) === fixture.start_block)?.block.hash,
    fixture.start_hash,
    `${fixture.name}: start hash`,
  )
  assert.equal(
    events.find((event) => Number(event.block.number) === fixture.end_block)?.block.hash,
    fixture.end_hash,
    `${fixture.name}: end hash`,
  )

  const anchors = fixture.anchor_events || (fixture.anchor_event ? [fixture.anchor_event] : [])
  for (const anchor of anchors) {
    assert.ok(
      events.some(
        (event) =>
          event.transaction.hash === anchor.transaction_hash &&
          event.transaction.index === anchor.transaction_index &&
          event.log.blockLogIndex === anchor.log_index,
      ),
      `${fixture.name}: anchor event`,
    )
  }

  console.log(
    `${fixture.name}: ${events.length} ordered events across ${blocks.length} blocks verified`,
  )
}

function snake(value) {
  return value.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toUpperCase()
}
