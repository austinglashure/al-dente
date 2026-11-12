// Milestone M1 evidence (docs/plan/phases/phase-01.md, WP1.10): for each
// acceptance repository, serve it, screenshot the first screen and a
// drill-down to a symbol, and save `aldente analyze --json`.
//
// Usage (after `cargo run -p acceptance`, which clones and pins the repos):
//   node scripts/milestone.mjs
// ALDENTE_BIN picks the binary (default ../target/release/aldente).

import { chromium } from '@playwright/test';
import { execFileSync, spawn } from 'node:child_process';
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const bin = process.env.ALDENTE_BIN ?? join(root, 'target', 'release', 'aldente');
const out = join(root, 'docs', 'milestones', 'm1');
mkdirSync(out, { recursive: true });

// Which aggregates to expand and which symbol to select, per repository.
const repos = [
  {
    name: 'ring',
    url: 'https://github.com/ring-clojure/ring',
    rev: '5f745bbc7daefa097d7e9cf6394b44dce51cf597',
    expand: ['unit:ring-core', 'clj:ring-core//ring.middleware.params'],
    select: 'clj:ring-core//ring.middleware.params/wrap-params',
  },
  {
    name: 'reitit',
    url: 'https://github.com/metosin/reitit',
    rev: '1d2fc6735f53e2026fed98e92322a29f7a35c371',
    expand: ['unit:modules/reitit-core', 'clj:modules/reitit-core//reitit.impl'],
    select: 'clj:modules/reitit-core//reitit.impl/resolve-routes',
  },
  {
    name: 're-frame',
    url: 'https://github.com/day8/re-frame',
    rev: '1a1bf1df6570b17a148ebce70d500ec2da393cdc',
    expand: ['unit:.', 'clj:.//re-frame.core'],
    select: 'clj:.//re-frame.core/dispatch',
  },
];

/** Starts `aldente serve` and resolves with its URL. */
function serve(repo) {
  const child = spawn(bin, ['serve', '--no-open', '--json', '--ref', repo.rev, repo.url], { stdio: ['ignore', 'pipe', 'inherit'] });
  return new Promise((ok, fail) => {
    let buf = '';
    child.stdout.on('data', (c) => {
      buf += c;
      const end = buf.indexOf('\n}\n');
      if (end >= 0) ok({ url: JSON.parse(buf.slice(0, end + 2)).data.url, child });
    });
    child.on('exit', (code) => fail(new Error(`serve exited ${code}`)));
  });
}

const browser = await chromium.launch({ channel: 'chromium' });
for (const repo of repos) {
  const json = execFileSync(bin, ['analyze', '--json', '--ref', repo.rev, repo.url], { encoding: 'utf8' });
  writeFileSync(join(out, `${repo.name}-analyze.json`), json);
  const { url, child } = await serve(repo);
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
  await page.route('**/*', (r) => (new URL(r.request().url()).hostname === '127.0.0.1' ? r.continue() : r.abort()));
  await page.goto(url);
  await page.locator('[data-testid="node-list"] li').first().waitFor();
  await page.waitForTimeout(2500);
  await page.screenshot({ path: join(out, `${repo.name}-1-overview.png`) });
  for (const id of repo.expand) {
    await page.locator(`[data-expand="${id}"]`).click();
    await page.locator(`[data-expand="${id}"]`).filter({ hasText: 'collapse' }).waitFor();
  }
  await page.locator(`[data-testid="node-list"] [data-node-id="${repo.select}"]`).click();
  await page.getByTestId('source').waitFor();
  await page.waitForTimeout(2500);
  await page.screenshot({ path: join(out, `${repo.name}-2-drill.png`) });
  await page.close();
  child.kill();
  console.log(`${repo.name}: saved`);
}
await browser.close();
