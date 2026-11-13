import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';
import { repoFile, startServer, startServerOn, type Server } from './server';

/** The node-list entry for a node ID. */
const listed = (page: Page, id: string) => page.locator(`[data-testid="node-list"] [data-node-id="${id}"]`);

/** Opens the details of `id` by searching for `query` and following the result. */
async function selectBySearch(page: Page, query: string, id: string) {
  await page.getByTestId('search').fill(query);
  await page.getByTestId('search').press('Enter');
  await page.locator(`[data-testid="results"] [data-node-id="${id}"]`).click();
  await expect(page.getByTestId('details')).toContainText(id.split('/').pop() ?? id);
}

test.describe('crater views on clj-mini', () => {
  let server: Server;
  test.beforeAll(async () => {
    server = await startServer('clj-mini');
  });
  test.afterAll(() => server.close());

  test('open a crater from a node, change direction, export the checklist', async ({ page }) => {
    await page.goto(server.url);
    await expect(listed(page, 'clj:.//shop.core')).toBeVisible();
    await selectBySearch(page, 'half-off', 'clj:.//shop.pricing/half-off');
    await page.getByTestId('details').locator('[data-action="crater-forward"]').click();

    const view = page.getByTestId('crater-view');
    await expect(view).toHaveAttribute('data-direction', 'forward');
    await expect(page.getByTestId('crater-summary')).toBeVisible();
    const forwardNodes = Number(await view.getAttribute('data-nodes'));
    expect(forwardNodes).toBeGreaterThan(0);

    await page.getByTestId('crater-direction').selectOption('reverse');
    await expect(view).toHaveAttribute('data-direction', 'reverse');
    await expect(page.getByTestId('crater-summary')).toBeVisible();
    await page.getByTestId('crater-tab-checklist').click();
    const checklist = page.getByTestId('checklist');
    await expect(checklist).toContainText('shop.pricing/half-off');
    await expect(checklist).toContainText('shop.pricing/price@:book');

    const csvDownload = page.waitForEvent('download');
    await page.getByTestId('export-csv').click();
    const csv = readFileSync(await (await csvDownload).path(), 'utf8');
    expect(csv.split('\r\n')[0]).toBe('location,symbol,distance,certainty,coverage,entry_point,id');
    expect(csv).toContain('src/shop/pricing.clj:9:1,shop.pricing/half-off,0,certain');

    const mdDownload = page.waitForEvent('download');
    await page.getByTestId('export-md').click();
    const md = readFileSync(await (await mdDownload).path(), 'utf8');
    expect(md).toContain('| `src/shop/pricing.clj:9:1` | shop.pricing/half-off | 0 | certain |');

    await page.getByTestId('crater-close').click();
    await expect(page.getByTestId('crater-view')).toHaveCount(0);
    await expect(page.getByTestId('canvas')).toBeVisible();
  });

  test('paste a trace, see its frames, and pin the crater to it', async ({ page }) => {
    await page.goto(server.url);
    await page.getByTestId('paste-trace').click();
    const trace = readFileSync(repoFile('fixtures/traces/jvm/clj-mini/checkout-book-nil-price.txt'), 'utf8');
    await page.getByTestId('trace-input').fill(trace);
    await page.getByTestId('trace-normalize').click();
    const frames = page.getByTestId('trace-frames');
    await expect(frames.locator('[data-testid="frame"][data-node-id="clj:.//shop.pricing/price@:book"]')).toHaveAttribute('data-match', 'exact');
    await expect(frames.getByTestId('noise-fold').first()).toBeVisible();

    await page.getByTestId('trace-pin').click();
    const view = page.getByTestId('crater-view');
    await expect(view).toHaveAttribute('data-direction', 'reverse');
    await expect(view).toHaveAttribute('data-pinned', '4');
    const path = page.getByTestId('pinned-path');
    await expect(path.locator('li')).toHaveCount(4);
    await expect(path.locator('[data-node-id="clj:.//shop.pricing/price@:book"]')).toBeVisible();
    await expect(path).not.toContainText('price@:food');
    // The pinned path is highlighted on the canvas.
    await expect(page.getByTestId('crater-canvas')).toHaveAttribute('data-highlighted', '4');
  });

  test('selecting a library lists its call sites and highlights its users', async ({ page }) => {
    await page.goto(server.url);
    await page.getByTestId('libraries').locator('summary').click();
    await page.locator('[data-lib-id="lib:npm:react"]').click();
    const panel = page.getByTestId('library-panel');
    await expect(panel).toContainText('lib:npm:react');
    await expect(panel.getByTestId('lib-site').first()).toContainText('src/shop/ui/views.cljs');
    await expect(page.getByTestId('canvas')).not.toHaveAttribute('data-highlighted', '0');
  });
});

test.describe('a large crater', () => {
  let server: Server;
  let dir: string;
  test.beforeAll(async () => {
    // 40 namespaces of 50 functions; function i calls 2i+1 and 2i+2 (a
    // binary tree of 2,000 functions across the namespaces).
    dir = mkdtempSync(join(tmpdir(), 'aldente-e2e-big-'));
    writeFileSync(join(dir, 'deps.edn'), '{:paths ["src"]}\n');
    mkdirSync(join(dir, 'src', 'big'), { recursive: true });
    const total = 2000;
    const ref = (from: number, to: number) => (Math.floor(from / 50) === Math.floor(to / 50) ? `f${to}` : `big.n${Math.floor(to / 50)}/f${to}`);
    for (let n = 0; n < 40; n++) {
      const needs = new Set<number>();
      for (let f = 0; f < 50; f++) {
        for (const c of [2 * (n * 50 + f) + 1, 2 * (n * 50 + f) + 2]) if (c < total && Math.floor(c / 50) !== n) needs.add(Math.floor(c / 50));
      }
      const req = needs.size ? `(:require ${[...needs].map((m) => `[big.n${m}]`).join(' ')})` : '';
      let src = `(ns big.n${n} ${req})\n`;
      for (let f = 49; f >= 0; f--) {
        const i = n * 50 + f;
        const calls = [2 * i + 1, 2 * i + 2].filter((c) => c < total).map((c) => `(${ref(i, c)} x)`);
        src = `${src}\n(defn f${i} [x] (do ${calls.join(' ')} x))\n`;
      }
      writeFileSync(join(dir, 'src', 'big', `n${n}.clj`), src);
    }
    server = await startServerOn(dir);
  });
  test.afterAll(() => {
    server.close();
    rmSync(dir, { recursive: true, force: true });
  });

  test('2,000 nodes lay out in a worker without freezing the page', async ({ page }) => {
    test.setTimeout(180_000);
    await page.goto(server.url);
    await page.evaluate(() => {
      const w = window as unknown as { longest: number };
      w.longest = 0;
      new PerformanceObserver((list) => {
        for (const e of list.getEntries()) w.longest = Math.max(w.longest, e.duration);
      }).observe({ type: 'longtask', buffered: false });
    });
    await selectBySearch(page, 'f0', 'clj:.//big.n0/f0');
    await page.getByTestId('details').locator('[data-action="crater-forward"]').click();
    await page.getByTestId('crater-unlimited').check();
    await expect(page.getByTestId('layout-status')).toBeVisible();
    // While ELK runs, the page still answers.
    await page.getByTestId('crater-tab-checklist').click();
    await page.getByTestId('crater-tab-graph').click();
    await expect(page.getByTestId('crater-summary')).toBeVisible({ timeout: 150_000 });
    // 2,000 functions plus the externals they call.
    expect(Number(await page.getByTestId('crater-view').getAttribute('data-nodes'))).toBeGreaterThanOrEqual(2000);
    const longest = await page.evaluate(() => (window as unknown as { longest: number }).longest);
    expect(longest).toBeLessThan(1000);
  });
});
