import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { causesSchema } from '../../src/lib/model.ts'
import { causeText } from '../../src/lib/outage.ts'

// Written by the Rust test `copy_fixture_matches_what_the_app_says`.
const fixture = JSON.parse(readFileSync(new URL('../../src-tauri/fixtures/outage-copy.json', import.meta.url), 'utf8')) as { name: string; causes: unknown; sentence: string | null }[]

test('History says where an outage broke in the same words as Rust', () => {
  assert.ok(fixture.length >= 10)
  for (const row of fixture) {
    assert.equal(causeText(causesSchema.parse(row.causes)), row.sentence, row.name)
  }
})

test('outages from before causes were kept show nothing extra', () => {
  assert.equal(causeText(undefined), null)
})
