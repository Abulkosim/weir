import AxeBuilder from '@axe-core/playwright'
import { expect, test } from '@playwright/test'

const views = [
  { name: 'never tested', url: '/?demo=1&state=never' },
  { name: 'result', url: '/?demo=1&state=result' },
  { name: 'stutter', url: '/?demo=1&state=stutter' },
  { name: 'testing', url: '/?demo=1&state=testing' },
  { name: 'failed', url: '/?demo=1&state=failed' },
  { name: 'offline', url: '/?demo=1&state=offline' },
  { name: 'history', url: '/?demo=1&page=history' },
  { name: 'settings', url: '/?demo=1&page=settings' },
]

for (const theme of ['light', 'dark'] as const) {
  for (const view of views) {
    test(`${view.name} has no serious accessibility issues in ${theme}`, async ({ page }) => {
      await page.goto(`${view.url}&theme=${theme}`)
      await page.evaluate(() => document.fonts.ready)
      await page.waitForTimeout(300)
      const results = await new AxeBuilder({ page }).include('.popover').analyze()
      const serious = results.violations.filter(violation => violation.impact === 'serious' || violation.impact === 'critical')
      expect(serious.map(violation => `${violation.id}: ${violation.nodes.map(node => node.target.join(' ')).join(', ')}`)).toEqual([])
    })
  }
}

test('the result view works from the keyboard alone', async ({ page, browserName }) => {
  const tab = browserName === 'webkit' ? 'Alt+Tab' : 'Tab'
  await page.goto('/?demo=1&state=stutter')
  await page.locator('body').focus()
  const order: string[] = []
  for (let step = 0; step < 4; step++) {
    await page.keyboard.press(tab)
    order.push(await page.evaluate(() => document.activeElement?.getAttribute('aria-label') || document.activeElement?.textContent?.trim() || ''))
  }
  expect(order).toEqual(['History', 'Settings', 'Calls may stutter: latency jumps to 180 ms under load', 'Test again'])
  await page.keyboard.press(`Shift+${tab}`)
  await page.keyboard.press('Enter')
  await expect(page.getByText('Lags under load: +166 ms')).toBeVisible()
})

test('keyboard focus shows a ring', async ({ page, browserName }) => {
  const tab = browserName === 'webkit' ? 'Alt+Tab' : 'Tab'
  await page.goto('/?demo=1&state=result')
  await page.getByRole('button', { name: 'Test again' }).focus()
  await page.keyboard.press(`Shift+${tab}`)
  await page.keyboard.press(tab)
  const outline = await page.getByRole('button', { name: 'Test again' }).evaluate(element => getComputedStyle(element).outlineStyle)
  expect(outline).toBe('solid')
})
