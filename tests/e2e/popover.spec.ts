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
      await expect(page.getByText('Download 312 megabits per second')).toBeVisible()
      await expect(page.getByText('Tested 12 min ago · Cloudflare, Frankfurt')).toBeVisible()
      await expect(page.getByText('62 ms under load')).toBeVisible()
      await expect(page.getByText('Uses about 250 MB')).toBeVisible()
      await expect(page.getByRole('button', { name: 'Good for 4K video and calls' })).toBeVisible()
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
      await expect(page.getByText(/megabits per second/)).toHaveCount(0)
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

for (const theme of themes) {
  test(`offline keeps the last result and disables Test in ${theme}`, async ({ page }, info) => {
    await page.goto(`/?demo=1&state=offline&theme=${theme}`)
    await expect(page.getByRole('status')).toContainText('No internet since')
    await expect(page.getByRole('button', { name: 'Test again' })).toBeDisabled()
    await expect(page.getByText('Download 312 megabits per second')).toBeVisible()
    await shoot(page, 'offline', info.outputPath(`offline-${theme}.png`))
  })
}

for (const theme of themes) {
  test(`plain meaning names the limit and opens the details in ${theme}`, async ({ page }, info) => {
    await page.goto(`/?demo=1&state=stutter&theme=${theme}`)
    const toggle = page.getByRole('button', { name: 'Calls may stutter: latency jumps to 180 ms under load' })
    await expect(toggle).toHaveAttribute('aria-expanded', 'false')
    await toggle.click()
    await expect(toggle).toHaveAttribute('aria-expanded', 'true')
    await expect(page.getByText('Lags under load: +166 ms')).toBeVisible()
    await expect(page.getByText('Used 238 MB')).toBeVisible()
    await expect(page.getByText('Good for 4K video', { exact: true })).toBeVisible()
    await shoot(page, 'stutter', info.outputPath(`stutter-${theme}.png`))
  })
}

const edges = [
  { state: 'stale', text: 'Tested 2 days ago · Cloudflare, Frankfurt', also: 'Over a day old. Test again for a current number.' },
  { state: 'capped', text: 'Stopped at the data limit, speed may be higher', also: 'Uses about 480 MB' },
  { state: 'varied', text: 'Varied 52–118 Mbps', also: 'Download 86 megabits per second' },
  { state: 'partial', text: 'Connection stalled', also: 'Not measured' },
] as const

for (const theme of themes) {
  for (const edge of edges) {
    test(`${edge.state} result is labelled in ${theme}`, async ({ page }, info) => {
      await page.goto(`/?demo=1&state=${edge.state}&theme=${theme}`)
      await expect(page.getByText(edge.text, { exact: true })).toBeVisible()
      await expect(page.getByText(edge.also, { exact: true }).first()).toBeAttached()
      await shoot(page, edge.state, info.outputPath(`${edge.state}-${theme}.png`))
    })
  }
}

const failures = {
  offline: 'No internet',
  refused: 'Cloudflare refused the test',
  serverError: 'Cloudflare server error',
  stalled: 'Connection stalled',
  tooShort: 'Test too short to measure',
  dropped: 'Connection kept dropping',
  networkChanged: 'Network changed during the test',
} as const

for (const [error, title] of Object.entries(failures)) {
  test(`failure ${error} has its own message`, async ({ page }) => {
    await page.goto(`/?demo=1&state=failed&error=${error}`)
    await expect(page.getByRole('alert').getByRole('heading')).toHaveText(title)
  })
}

for (const theme of themes) {
  test(`history lists tests by day with a summary in ${theme}`, async ({ page }, info) => {
    await page.goto(`/?demo=1&theme=${theme}`)
    await page.getByRole('button', { name: 'History' }).click()
    await expect(page.getByRole('heading', { name: 'History' })).toBeVisible()
    await expect(page.getByText('Mbps median')).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Today' })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Yesterday' })).toBeVisible()
    await expect(page.getByRole('listitem').filter({ hasText: 'Connection stalled' })).toHaveCount(1)
    await expect(page.getByText('Download 312 Mbps', { exact: true })).toBeVisible()
    const height = Number(await page.locator('.app-shell').getAttribute('data-fit-height'))
    expect(height).toBeLessThanOrEqual(600)
    await shoot(page, 'history', info.outputPath(`history-${theme}.png`))
    await page.keyboard.press('Escape')
    await expect(page.getByRole('button', { name: 'Test again' })).toBeVisible()
  })
}

test('the provider report saves from History and Settings', async ({ page }) => {
  await page.goto('/?demo=1&page=history')
  await page.getByRole('button', { name: 'Report' }).click()
  await expect(page.getByRole('status').filter({ hasText: /Saved weir-report-\d{4}-\d{2}-\d{2}\.pdf/ })).toBeVisible()
  await page.goto('/?demo=1&page=settings')
  await expect(page.getByText('A PDF of the last 30 days', { exact: false })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Save PDF' })).toBeEnabled()
})

test('export confirms the saved file name', async ({ page }) => {
  await page.goto('/?demo=1&page=history')
  await page.getByRole('button', { name: 'Export CSV' }).click()
  await expect(page.getByRole('status').filter({ hasText: /Saved weir-history-\d{4}-\d{2}-\d{2}\.csv/ })).toBeVisible()
})

for (const theme of themes) {
  test(`settings save each change and switch the theme in ${theme}`, async ({ page }, info) => {
    await page.goto(`/?demo=1&state=result`)
    await page.emulateMedia({ colorScheme: theme })
    await page.getByRole('button', { name: 'Settings' }).click()
    await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible()
    await expect(page.getByText('14 tests')).toBeVisible()
    await shoot(page, 'settings', info.outputPath(`settings-${theme}.png`))
    const other = theme === 'light' ? 'Dark' : 'Light'
    await page.getByRole('button', { name: other }).click()
    await expect(page.locator('html')).toHaveAttribute('data-theme', other.toLowerCase())
    await page.getByRole('switch', { name: 'Launch at login' }).click()
    await expect(page.getByRole('switch', { name: 'Launch at login' })).toHaveAttribute('aria-checked', 'false')
  })
}

test('clearing history asks first', async ({ page }) => {
  await page.goto('/?demo=1&page=settings')
  await page.getByRole('button', { name: 'Clear…' }).click()
  const dialog = page.getByRole('alertdialog')
  await expect(dialog).toContainText('Deletes all 14 saved tests.')
  await dialog.getByRole('button', { name: 'Keep history' }).click()
  await expect(page.getByText('14 tests')).toBeVisible()
  await page.getByRole('button', { name: 'Clear…' }).click()
  await page.getByRole('alertdialog').getByRole('button', { name: 'Clear history' }).click()
  await expect(page.getByText('0 tests')).toBeVisible()
})

test('an available update shows a dot and an install button in Settings', async ({ page }, info) => {
  await page.goto('/?demo=1&update=update-available')
  await page.getByRole('button', { name: 'Settings, update available' }).click()
  await expect(page.getByText('Check for updates once a day', { exact: false }).or(page.getByText(/0\.9\.0/)).first()).toBeVisible()
  await page.locator('.settings-content').screenshot({ path: info.outputPath('update-available.png') })
})

test('history button stays hidden until there is history', async ({ page }) => {
  await page.goto('/?demo=1&state=never')
  await expect(page.getByRole('button', { name: 'History' })).toHaveCount(0)
})

const checks = [
  { state: 'check-fine', headline: 'Everything looks fine from here.' },
  { state: 'check-isp', headline: 'Your Wi-Fi is fine. The problem is past your router.' },
  { state: 'check-router', headline: 'Your router isn’t answering. Restart it, or check the cable.' },
  { state: 'check-wifi', headline: 'Your Wi-Fi signal is weak (-81 dBm). Move closer to the router.' },
  { state: 'check-dns', headline: 'Websites are slow to open: looking up names takes 450 ms.' },
] as const

for (const theme of themes) {
  for (const check of checks) {
    test(`connection check ${check.state} reads as one sentence in ${theme}`, async ({ page }, info) => {
      await page.goto(`/?demo=1&state=${check.state}&theme=${theme}`)
      await expect(page.getByRole('heading', { name: 'Connection' })).toBeVisible()
      await expect(page.getByRole('status').filter({ hasText: check.headline })).toBeVisible()
      await expect(page.locator('.check-step')).toHaveCount(4)
      await expect(page.getByRole('button', { name: 'Check again' })).toBeEnabled()
      await shoot(page, check.state, info.outputPath(`${check.state}-${theme}.png`))
    })
  }
}

test('check connection runs from the result and shows progress first', async ({ page }) => {
  await page.goto('/?demo=1&state=result')
  await page.getByRole('button', { name: 'Check connection' }).click()
  await expect(page.getByRole('status').filter({ hasText: 'Checking your connection' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Check' })).toBeDisabled()
  await expect(page.getByRole('status').filter({ hasText: 'Everything looks fine from here.' })).toBeVisible({ timeout: 4_000 })
  await page.keyboard.press('Escape')
  await expect(page.getByRole('button', { name: 'Test again' })).toBeVisible()
})

test('failed and offline states offer the connection check', async ({ page }) => {
  await page.goto('/?demo=1&state=failed')
  await expect(page.getByRole('button', { name: 'Check connection' }).first()).toBeVisible()
  await page.goto('/?demo=1&state=offline')
  await page.getByRole('button', { name: 'Check', exact: true }).click()
  await expect(page.getByRole('heading', { name: 'Connection' })).toBeVisible()
})

test('an outage found by the watch shows like offline but keeps Test usable', async ({ page }) => {
  await page.goto('/?demo=1&state=outage')
  await expect(page.getByRole('status').filter({ hasText: /No internet since/ })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Test again' })).toBeEnabled()
})

test('history shows download by hour of day', async ({ page }) => {
  await page.goto('/?demo=1&page=history')
  await expect(page.getByText('Download by hour of day')).toBeVisible()
  await expect(page.locator('.hour-bar')).toHaveCount(24)
  await expect(page.locator('.hours-card .sub')).toContainText(/Slowest around|No hour stands out/)
})

test('outages appear in history with their length', async ({ page }) => {
  await page.goto('/?demo=1&page=history')
  await expect(page.getByText(/No internet until .* · 7 min/)).toBeVisible()
})

test('scheduled tests show their data cost and are marked in the result and history', async ({ page }) => {
  await page.goto('/?demo=1&page=settings')
  const schedule = page.getByRole('combobox', { name: 'Test automatically' })
  await expect(schedule).toHaveText(/Every 12 hours/)
  await expect(page.getByText(/About 250 MB a test, 15 GB a month · Next test/)).toBeVisible()
  await schedule.click()
  await page.getByRole('option', { name: 'Off' }).click()
  await expect(page.getByText(/GB a month/)).toHaveCount(0)
  await page.goto('/?demo=1&state=scheduled')
  await expect(page.locator('.where')).toContainText('Scheduled test, Tested 12 min ago')
  await page.goto('/?demo=1&page=history')
  await expect(page.locator('.history-scheduled').first()).toBeVisible()
})

test('the result compares with the plan speed and settings change it', async ({ page }) => {
  await page.goto('/?demo=1&state=result')
  await expect(page.getByText('62% of your 500 Mbps plan')).toBeVisible()
  await expect(page.getByText('82% of 50 Mbps plan')).toBeVisible()
  await page.getByRole('button', { name: 'Settings' }).click()
  const download = page.getByLabel('Download speed in Mbps')
  await download.fill('fast')
  await download.press('Enter')
  await expect(page.getByRole('alert')).toHaveText('Enter a whole number of Mbps, or leave it empty.')
  await download.fill('1000')
  await download.press('Enter')
  await expect(page.getByRole('alert')).toHaveCount(0)
  await page.getByRole('button', { name: 'Back' }).click()
  await expect(page.getByText('31% of your 1000 Mbps plan')).toBeVisible()
  await page.getByRole('button', { name: 'Settings' }).click()
  await page.getByLabel('Download speed in Mbps').fill('')
  await page.getByLabel('Download speed in Mbps').press('Tab')
  await page.getByRole('button', { name: 'Back' }).click()
  await expect(page.getByText(/% of your .* plan/)).toHaveCount(0)
})

test('without a plan the result stays as it was', async ({ page }) => {
  await page.goto('/?demo=1&state=noplan')
  await expect(page.getByText('Download 312 megabits per second')).toBeVisible()
  await expect(page.locator('.plan-share')).toHaveCount(0)
})

test('history shows the median against the plan', async ({ page }) => {
  await page.goto('/?demo=1&page=history')
  await expect(page.getByText(/^\d+% of your 500 Mbps plan$/)).toBeVisible()
})

test('outage settings explain the cost and gate notifications', async ({ page }) => {
  await page.goto('/?demo=1&page=settings')
  const watch = page.getByRole('switch', { name: 'Watch for outages' })
  await expect(page.getByText('A tiny check about once a minute, under 2 kB.', { exact: false })).toBeVisible()
  await expect(watch).toHaveAttribute('aria-checked', 'true')
  await watch.click()
  await expect(page.getByRole('switch', { name: 'Notify when it drops and returns' })).toBeDisabled()
})

test('keyboard shortcuts run a test and open the connection check', async ({ page }) => {
  await page.clock.install()
  await page.goto('/?demo=1&state=result')
  await page.keyboard.press('ControlOrMeta+k')
  await expect(page.getByRole('heading', { name: 'Connection' })).toBeVisible()
  await page.keyboard.press('Escape')
  await page.keyboard.press('ControlOrMeta+t')
  await page.clock.runFor(1_000)
  await expect(page.getByRole('status').filter({ hasText: 'Measuring latency' })).toBeVisible()
})

test('copy summary puts the result on the clipboard', async ({ page, context, browserName }) => {
  if (browserName === 'chromium') await context.grantPermissions(['clipboard-read', 'clipboard-write'])
  await page.goto('/?demo=1&state=result')
  await page.getByRole('button', { name: 'Good for 4K video and calls' }).click()
  await page.getByRole('button', { name: 'Copy summary' }).click()
  await expect(page.getByRole('status').filter({ hasText: /Copied the summary|Couldn’t copy/ })).toBeVisible()
  if (browserName === 'chromium') {
    const text = await page.evaluate(() => navigator.clipboard.readText())
    expect(text).toContain('Download 312 Mbps · Upload 41 Mbps')
    expect(text).toContain('Cloudflare, Frankfurt')
  }
})

test('icon buttons explain themselves with their shortcut', async ({ page }) => {
  await page.goto('/?demo=1&state=result')
  await page.getByRole('button', { name: 'Check connection' }).hover()
  await expect(page.getByRole('tooltip')).toHaveText(/Check connection \((⌘|Ctrl\+)K\)/)
})

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
  await expect(page.getByText('Download 312 megabits per second')).toBeVisible()
})

test('cancel returns to the previous state without a new result', async ({ page }) => {
  await page.goto('/?demo=1&state=testing')
  await page.getByRole('button', { name: 'Cancel' }).click()
  await expect(page.getByRole('button', { name: 'Test again' })).toBeVisible()
  await expect(page.getByText('12 min ago')).toBeVisible()
})

for (const state of ['result', 'stutter', 'never', 'testing'] as const) {
  test(`a short native window grows to fit the ${state} view instead of clipping it`, async ({ page }) => {
    await page.setViewportSize({ width: 380, height: 200 })
    await page.goto(`/?demo=1&frame=1&state=${state}`)
    const shell = page.locator('.app-shell')
    await page.evaluate(() => document.fonts.ready)
    let fitted = 0
    await expect.poll(async () => {
      const next = Number(await shell.getAttribute('data-fit-height'))
      const settled = next === fitted && next > 200
      fitted = next
      return settled
    }).toBe(true)
    expect(fitted).toBeLessThanOrEqual(600)
    await page.setViewportSize({ width: 380, height: fitted })
    await expect.poll(async () => Number(await shell.getAttribute('data-fit-height'))).toBe(fitted)
    const main = page.locator('.popover > main')
    await expect.poll(() => main.evaluate(element => element.scrollHeight - element.clientHeight)).toBeLessThanOrEqual(1)
    const button = page.getByRole('button', { name: /^(Test|Test again|Cancel)$/ })
    await expect(button).toBeInViewport({ ratio: 1 })
  })
}

test('history taller than the popover scrolls inside it', async ({ page }) => {
  await page.setViewportSize({ width: 380, height: 600 })
  await page.goto('/?demo=1&frame=1&page=history')
  const shell = page.locator('.app-shell')
  await expect.poll(async () => Number(await shell.getAttribute('data-fit-height'))).toBe(600)
  const main = page.locator('.popover > main')
  expect(await main.evaluate(element => element.scrollHeight > element.clientHeight)).toBe(true)
  await main.evaluate(element => element.scrollTo(0, element.scrollHeight))
  await expect(page.getByRole('heading', { name: 'History' })).toBeInViewport()
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
