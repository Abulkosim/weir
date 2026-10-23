import { expect, test, type Page } from '@playwright/test'

const themes = ['light', 'dark'] as const

async function shoot(page: Page, name: string, path: string): Promise<void> {
  await page.evaluate(() => document.fonts.ready)
  await page.locator('.app-shell').screenshot({ path, animations: 'disabled' })
  void name
}

for (const theme of themes) {
  test.describe(`${theme} theme`, () => {
    test('never tested offers the first test with its data cost', async ({ page }, info) => {
      await page.goto(`/?demo=1&state=never&theme=${theme}`)
      await expect(page.getByRole('heading', { name: 'Not tested yet' })).toBeVisible()
      await expect(page.getByText('Uses up to 600 MB')).toBeVisible()
      await expect(page.getByRole('button', { name: 'Test', exact: true })).toBeEnabled()
      await shoot(page, 'never', info.outputPath(`never-${theme}.png`))
    })

    test('result shows every metric with server and age', async ({ page }, info) => {
      await page.goto(`/?demo=1&state=result&theme=${theme}`)
      await expect(page.getByLabel('Download 312 megabits per second')).toBeVisible()
      await expect(page.getByText('Cloudflare, Frankfurt · 12 min ago')).toBeVisible()
      await expect(page.getByText('62 ms under load')).toBeVisible()
      await expect(page.getByText('Uses about 250 MB')).toBeVisible()
      await expect(page.getByRole('button', { name: 'Test again' })).toBeVisible()
      await shoot(page, 'result', info.outputPath(`result-${theme}.png`))
    })

    for (const state of ['testing', 'testing-latency', 'testing-upload'] as const) {
      test(`${state} shows live progress and a way out`, async ({ page }, info) => {
        await page.goto(`/?demo=1&state=${state}&theme=${theme}`)
        await expect(page.getByRole('button', { name: 'Cancel' })).toBeVisible()
        await expect(page.getByText(/Used .* so far/)).toBeVisible()
        await expect(page.getByRole('status')).toHaveText(/Measuring (latency|download|upload)/)
        await shoot(page, state, info.outputPath(`${state}-${theme}.png`))
      })
    }

    test('failed test says why and keeps the last result separate', async ({ page }, info) => {
      await page.goto(`/?demo=1&state=failed&theme=${theme}`)
      await expect(page.getByRole('alert')).toContainText('Connection stalled')
      await expect(page.getByText(/Last result 312 Mbps down · 3 h ago/)).toBeVisible()
      await expect(page.getByLabel(/megabits per second/)).toHaveCount(0)
      await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible()
      await shoot(page, 'failed', info.outputPath(`failed-${theme}.png`))
    })
  })
}

for (const theme of themes) {
  test(`metered connection asks before testing in ${theme}`, async ({ page }, info) => {
    await page.clock.install()
    await page.goto(`/?demo=1&state=metered&theme=${theme}`)
    await expect(page.getByText('Hotspot · Uses about 250 MB')).toBeVisible()
    await page.getByRole('button', { name: 'Test again' }).click()
    const dialog = page.getByRole('alertdialog')
    await expect(dialog).toContainText('You’re on a hotspot. Test anyway?')
    await expect(dialog).toContainText('Uses about 250 MB of your data.')
    await expect(page.getByRole('button', { name: 'Not now' })).toBeFocused()
    await shoot(page, 'metered', info.outputPath(`metered-${theme}.png`))
    await page.keyboard.press('Escape')
    await expect(dialog).toHaveCount(0)
    await page.getByRole('button', { name: 'Test again' }).click()
    await page.getByRole('button', { name: 'Test anyway' }).click()
    await page.clock.runFor(1_000)
    await expect(page.getByRole('status')).toHaveText('Measuring latency')
  })
}

test('a simulated run goes from Test to a settled result', async ({ page }) => {
  await page.clock.install()
  await page.goto('/?demo=1&state=never')
  await page.getByRole('button', { name: 'Test', exact: true }).click()
  await page.clock.runFor(1_000)
  await expect(page.getByRole('status')).toHaveText('Measuring latency')
  await page.clock.runFor(3_000)
  await expect(page.getByRole('status')).toHaveText('Measuring download')
  await page.clock.runFor(6_000)
  await expect(page.getByRole('status')).toHaveText('Measuring upload')
  await page.clock.runFor(5_000)
  await expect(page.getByLabel('Download 312 megabits per second')).toBeVisible()
})

test('cancel returns to the previous state without a new result', async ({ page }) => {
  await page.goto('/?demo=1&state=testing')
  await page.getByRole('button', { name: 'Cancel' }).click()
  await expect(page.getByRole('button', { name: 'Test again' })).toBeVisible()
  await expect(page.getByText('12 min ago')).toBeVisible()
})

test('shell reports a fitted height within the popover bounds', async ({ page }) => {
  await page.goto('/?demo=1')
  const shell = page.locator('.app-shell')
  await expect(shell).not.toHaveAttribute('data-fit-height', '0')
  const height = Number(await shell.getAttribute('data-fit-height'))
  expect(height).toBeGreaterThan(200)
  expect(height).toBeLessThanOrEqual(600)
})

for (const theme of themes) {
  test(`design gallery renders every control in ${theme}`, async ({ page, browserName }, info) => {
    await page.setViewportSize({ width: 420, height: 900 })
    await page.goto(`/?demo=1&state=design&theme=${theme}`)
    await expect(page.getByRole('switch')).toBeVisible()
    await expect(page.getByRole('combobox')).toBeVisible()
    await page.getByRole('button', { name: 'Test' }).focus()
    await page.keyboard.press(browserName === 'webkit' ? 'Alt+Tab' : 'Tab')
    await expect(page.getByRole('button', { name: 'Cancel' })).toBeFocused()
    await shoot(page, 'design', info.outputPath(`design-${theme}.png`))
  })
}
