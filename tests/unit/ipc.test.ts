import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { historySchema, viewSchema } from '../../src/lib/model.ts'

// Written by the Rust test `ipc_fixture_matches_what_the_app_sends`.
const fixture = JSON.parse(readFileSync(new URL('../../src-tauri/fixtures/ipc.json', import.meta.url), 'utf8'))

test('the schemas read every field Rust sends, and drop none', () => {
  assert.deepEqual(viewSchema.parse(fixture.view), fixture.view)
  assert.deepEqual(historySchema.parse(fixture.history), fixture.history)
})
