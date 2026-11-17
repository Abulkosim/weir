import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { causesSchema } from '../../src/lib/model.ts'
import { causeText } from '../../src/lib/outage.ts'

// Written by the Rust test `copy_fixture_matches_the_wording` in `core/outage`.
const cases: { name: string; causes: unknown; sentence: string | null }[] = JSON.parse(readFileSync(new URL('../../src-tauri/fixtures/outage-copy.json', import.meta.url), 'utf8'))

test('outage causes read the same as in Rust', () => {
  assert.ok(cases.length >= 10)
  for (const { name, causes, sentence } of cases) assert.equal(causeText(causesSchema.parse(causes)), sentence, name)
})

test('an outage from before causes were kept says nothing more', () => {
  assert.equal(causeText(undefined), null)
})
