import { expect, test } from '@playwright/test'

for (const theme of ['light', 'dark'] as const) {
  test(`never-tested popover renders in ${theme}`, async ({ page }, info) => {
    await page.goto(`/?demo=1&theme=${theme}`)
    await expect(page.getByRole('heading', { name: 'Not tested yet' })).toBeVisible()
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme)
    await page.locator('.app-shell').screenshot({ path: info.outputPath(`never-tested-${theme}.png`) })
  })
}

test('shell reports a fitted height within the popover bounds', async ({ page }) => {
  await page.goto('/?demo=1')
  const shell = page.locator('.app-shell')
  await expect(shell).not.toHaveAttribute('data-fit-height', '0')
  const height = Number(await shell.getAttribute('data-fit-height'))
  expect(height).toBeGreaterThan(80)
  expect(height).toBeLessThanOrEqual(600)
})

for (const theme of ['light', 'dark'] as const) {
  test(`design gallery renders every control in ${theme}`, async ({ page, browserName }, info) => {
    await page.setViewportSize({ width: 420, height: 900 })
    await page.goto(`/?demo=1&state=design&theme=${theme}`)
    await expect(page.getByRole('switch')).toBeVisible()
    await expect(page.getByRole('combobox')).toBeVisible()
    await page.getByRole('button', { name: 'Test' }).focus()
    await page.keyboard.press(browserName === 'webkit' ? 'Alt+Tab' : 'Tab')
    await expect(page.getByRole('button', { name: 'Cancel' })).toBeFocused()
    await page.locator('.app-shell').screenshot({ path: info.outputPath(`design-${theme}.png`) })
  })
}
