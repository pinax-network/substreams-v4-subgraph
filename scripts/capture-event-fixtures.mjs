#!/usr/bin/env node

import assert from 'node:assert/strict'
import { readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const ranges = JSON.parse(await readFile(resolve(root, 'fixtures/base-ranges.json'), 'utf8'))
const lock = JSON.parse(
  await readFile(
    resolve(
      root,
      'artifacts/deployment/Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB/artifacts.lock.json',
    ),
    'utf8',
  ),
)
const rpcUrl = process.env.BASE_RPC_URL || 'https://mainnet.base.org'
const output = resolve(root, 'fixtures/event-logs.json')
const addresses = lock.data_sources.map(({ address }) => address.toLowerCase())
const topicToEvent = new Map(
  Object.entries(ranges.event_topics).map(([signature, topic]) => [
    topic,
    signature.slice(0, signature.indexOf('(')),
  ]),
)
const requiredEvents = [...new Set(topicToEvent.values())]
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
      const body = await response.json()
      if (body.error) throw new Error(JSON.stringify(body.error))
      return body.result
    } catch (error) {
      lastError = error
      if (attempt < 5) await new Promise((accept) => setTimeout(accept, attempt * 250))
    }
  }
  throw lastError
}

function quantity(value) {
  return Number(BigInt(value))
}

function blockTag(number) {
  return `0x${number.toString(16)}`
}

const representatives = new Map()
for (const range of ranges.ranges) {
  const logs = await rpc('eth_getLogs', [
    {
      fromBlock: blockTag(range.start_block),
      toBlock: blockTag(range.end_block),
      address: lock.data_sources.map(({ address }) => address),
      topics: [[...topicToEvent.keys()]],
    },
  ])
  for (const log of logs) {
    const event = topicToEvent.get(log.topics[0])
    if (event && !representatives.has(event)) representatives.set(event, log)
  }
}

assert.deepEqual([...representatives.keys()].sort(), requiredEvents.sort(), 'all events represented')
const transactionHashes = [...new Set([...representatives.values()].map((log) => log.transactionHash))]
const blocks = new Map()

for (const transactionHash of transactionHashes) {
  const receipt = await rpc('eth_getTransactionReceipt', [transactionHash])
  const blockNumber = quantity(receipt.blockNumber)
  let fixtureBlock = blocks.get(blockNumber)
  if (!fixtureBlock) {
    const block = await rpc('eth_getBlockByNumber', [receipt.blockNumber, false])
    fixtureBlock = {
      number: blockNumber,
      hash: block.hash,
      parent_hash: block.parentHash,
      timestamp_seconds: quantity(block.timestamp),
      transactions: [],
    }
    blocks.set(blockNumber, fixtureBlock)
  }

  const logs = receipt.logs
    .map((log, transactionLogIndex) => ({ log, transactionLogIndex }))
    .filter(
      ({ log }) =>
        addresses.includes(log.address.toLowerCase()) && topicToEvent.has(log.topics[0]),
    )
    .map(({ log, transactionLogIndex }) => ({
      event: topicToEvent.get(log.topics[0]),
      address: log.address,
      topics: log.topics,
      data: log.data,
      transaction_log_index: transactionLogIndex,
      block_log_index: quantity(log.logIndex),
    }))

  fixtureBlock.transactions.push({
    index: quantity(receipt.transactionIndex),
    hash: receipt.transactionHash,
    from: receipt.from,
    to: receipt.to,
    status: quantity(receipt.status),
    logs,
  })
}

const fixture = {
  format_version: 1,
  network: ranges.network,
  deployment: ranges.deployment,
  source: 'Base JSON-RPC receipts for pinned canonical blocks',
  blocks: [...blocks.values()]
    .sort((a, b) => a.number - b.number)
    .map((block) => ({
      ...block,
      transactions: block.transactions.sort((a, b) => a.index - b.index),
    })),
}

await writeFile(output, `${JSON.stringify(fixture, null, 2)}\n`)
console.log(
  `wrote ${output} with ${fixture.blocks.length} blocks, ${transactionHashes.length} transactions, and ${requiredEvents.length} event kinds`,
)
