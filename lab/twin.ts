// Runs Cloudflare's own speed test library in headless Chromium against the lab
// server. Any request to another origin is aborted and counted.
// node lab/twin.ts --url http://<server>/twin/index.html [--smoke]
// Prints one JSON line with numbers, error strings and the blocked count only.

import { chromium } from '@playwright/test';

const args = process.argv.slice(2);
const url = args[args.indexOf('--url') + 1] ?? '';
const smoke = args.includes('--smoke');
const WAIT_MS = 160_000;

async function main() {
  const origin = new URL(url).origin;
  const browser = await chromium.launch({
    args: ['--disable-background-networking', '--disable-component-update', '--no-proxy-server'],
  });
  let blocked = 0;
  try {
    const context = await browser.newContext();
    await context.route('**/*', (route) => {
      if (new URL(route.request().url()).origin === origin) return route.continue();
      blocked += 1;
      return route.abort('blockedbyclient');
    });
    const page = await context.newPage();
    await page.goto(url, { waitUntil: 'load', timeout: 20_000 });
    await page.waitForFunction(() => document.getElementById('state')?.textContent === 'ready', null, { timeout: 20_000 });
    if (smoke) {
      console.log(JSON.stringify({ ready: true, blocked }));
      return;
    }
    const began = Date.now();
    await page.evaluate(() => window.__twinStart?.());
    await page
      .waitForFunction(() => window.__twin !== undefined, null, { timeout: WAIT_MS, polling: 500 })
      .catch(() => undefined);
    const twin = await page.evaluate(() => window.__twin ?? null);
    const errors = await page.evaluate(() => window.__twinErrors.map((message) => message.replace(/https?:\/\/[^\s)]+/g, '<url>')));
    if (!twin && !errors.length) errors.push('timeout');
    console.log(JSON.stringify({ ...(twin ?? {}), errors, blocked, wallMs: Date.now() - began }));
  } finally {
    await browser.close();
  }
}

main().catch((error: unknown) => {
  console.log(JSON.stringify({ errors: [String(error).replace(/https?:\/\/[^\s)]+/g, '<url>').slice(0, 300)], blocked: 0 }));
  process.exit(1);
});
