import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const pages = await readFile(new URL('../src/pages.tsx', import.meta.url), 'utf8')
const styles = await readFile(new URL('../src/index.css', import.meta.url), 'utf8')

test('Nginx site tab panels are not nested inside a tab-hidden overview panel', () => {
  assert.doesNotMatch(pages, /data-nginx-site-panel="overview-grid"/)
  assert.match(pages, /<div className="contents"><div data-nginx-site-panel="overview-cards"/)

  for (const tab of ['overview', 'config', 'domains', 'operations', 'activity']) {
    assert.ok(styles.includes(`.nginx-site-tab-panels[data-active-tab="${tab}"] [data-nginx-site-panel="${tab}"]`))
  }
})

