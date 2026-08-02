#!/usr/bin/env node

import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const fixtureFile = resolve(repositoryRoot, 'fixtures/base-ranges.json')
const artifactLockFile = resolve(
  repositoryRoot,
  'artifacts/deployment/Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB/artifacts.lock.json',
)
const rpcUrl = process.env.BASE_RPC_URL || 'https://mainnet.base.org'

const fixtures = JSON.parse(await readFile(fixtureFile, 'utf8'))
const artifactLock = JSON.parse(await readFile(artifactLockFile, 'utf8'))
const addresses = artifactLock.data_sources.map(({ address }) => address)
const topicToEvent = new Map(
  Object.entries(fixtures.event_topics).map(([signature, topic]) => [
    topic,
    signature.slice(0, signature.indexOf('(')),
  ]),
)

let requestId = 0
async function rpc(method, params) {
  let lastError
  for (let attempt = 1; attempt <= 5; attempt += 1) {
    try {
      const response = await fetch(rpcUrl, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: ++requestId, method, params }),
      })
      assert.equal(response.ok, true, `HTTP ${response.status}`)
      const result = await response.json()
      if (result.error) throw new Error(JSON.stringify(result.error))
      return result.result
    } catch (error) {
      lastError = error
      if (attempt < 5) await new Promise((accept) => setTimeout(accept, attempt * 250))
    }
  }
  throw lastError
}

function blockTag(number) {
  return `0x${number.toString(16)}`
}

for (const fixture of fixtures.ranges) {
  const [startBlock, endBlock, logs] = await Promise.all([
    rpc('eth_getBlockByNumber', [blockTag(fixture.start_block), false]),
    rpc('eth_getBlockByNumber', [blockTag(fixture.end_block), false]),
    rpc('eth_getLogs', [
      {
        fromBlock: blockTag(fixture.start_block),
        toBlock: blockTag(fixture.end_block),
        address: addresses,
        topics: [[...topicToEvent.keys()]],
      },
    ]),
  ])

  assert(startBlock, `${fixture.name}: start block not found`)
  assert(endBlock, `${fixture.name}: end block not found`)
  assert.equal(startBlock.hash, fixture.start_hash, `${fixture.name}: start hash`)
  assert.equal(endBlock.hash, fixture.end_hash, `${fixture.name}: end hash`)

  const counts = Object.fromEntries(Object.keys(fixture.expected_events).map((event) => [event, 0]))
  for (const log of logs) {
    const event = topicToEvent.get(log.topics[0])
    assert(event, `${fixture.name}: unexpected topic ${log.topics[0]}`)
    counts[event] += 1
  }
  assert.deepEqual(counts, fixture.expected_events, `${fixture.name}: event counts`)

  const anchors = fixture.anchor_events || (fixture.anchor_event ? [fixture.anchor_event] : [])
  for (const anchor of anchors) {
    assert(
      logs.some(
        (log) =>
          log.transactionHash === anchor.transaction_hash &&
          Number(BigInt(log.transactionIndex)) === anchor.transaction_index &&
          Number(BigInt(log.logIndex)) === anchor.log_index,
      ),
      `${fixture.name}: anchor event not found`,
    )
  }

  console.log(`${fixture.name}: ${logs.length} events and boundary hashes verified`)
}
