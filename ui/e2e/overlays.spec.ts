import { expect, test } from '@playwright/test';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { startServer, startServerOn, type Server } from './server';

test.describe('overlays, hotspots and measures', () => {
  let server: Server;
  test.beforeAll(async () => {
    server = await startServer('clj-mini');
  });
  test.afterAll(() => server.close());

  test('switching to coverage styles untested nodes, without a re-layout or touching edges', async ({ page }) => {
    await page.goto(server.url);
    await page.locator('[data-open="clj:.//shop.util"]').click();
    const canvas = page.getByTestId('canvas');
    await expect(page.locator('[data-testid="node-list"] [data-node-id="clj:.//shop.util/log-timing"]')).toBeVisible();
    await expect(canvas).toHaveAttribute('data-overlay', 'risk');
    await expect(canvas).toHaveAttribute('data-styled-untested', '0');
    // Hide possible edges first: overlays must respect the certainty filter.
    await page.getByTestId('show-possible').uncheck();
    await page.waitForTimeout(500);
    const edges = await canvas.getAttribute('data-visible-edges');
    const runs = await canvas.getAttribute('data-layout-runs');
    await page.getByTestId('overlay').selectOption('coverage');
    await expect(canvas).toHaveAttribute('data-overlay', 'coverage');
    // log-timing and with-timing are untested; clamp and apply-discount indirect.
    expect(Number(await canvas.getAttribute('data-styled-untested'))).toBeGreaterThan(0);
    await expect(canvas).toHaveAttribute('data-visible-edges', edges ?? '');
    await expect(canvas).toHaveAttribute('data-layout-runs', runs ?? '');
    await page.getByTestId('overlay').selectOption('none');
    await expect(canvas).toHaveAttribute('data-styled-untested', '0');
    await expect(canvas).toHaveAttribute('data-layout-runs', runs ?? '');
  });

  test('cycles list says when there are none', async ({ page }) => {
    await page.goto(server.url);
    await page.getByTestId('cycles').locator('summary').click();
    await expect(page.getByTestId('cycles')).toContainText('No cycles at this level.');
  });
});

test.describe('show this cycle', () => {
  let server: Server;
  test.beforeAll(async () => {
    const dir = mkdtempSync(join(tmpdir(), 'aldente-cycle-'));
    mkdirSync(join(dir, 'src', 'c'), { recursive: true });
    writeFileSync(join(dir, 'deps.edn'), '{:paths ["src"]}\n');
    writeFileSync(join(dir, 'src', 'c', 'core.clj'), '(ns c.core)\n(declare g)\n(defn f [n] (g n))\n(defn g [n] (if (pos? n) (f (dec n)) 0))\n(defn -main [] (f 3))\n');
    server = await startServerOn(dir);
  });
  test.afterAll(() => server.close());

  test('isolates the cycle on the canvas', async ({ page }) => {
    await page.goto(server.url);
    await page.getByTestId('cycles').locator('summary').click();
    await page.getByTestId('cycles-level').selectOption('symbol');
    await page.locator('[data-show-cycle="0"]').click();
    await expect(page.getByTestId('breadcrumb')).toContainText('cycle of 2');
    const list = page.getByTestId('node-list');
    await expect(list.locator('[data-node-id="clj:.//c.core/f"]')).toBeVisible();
    await expect(list.locator('[data-node-id="clj:.//c.core/g"]')).toBeVisible();
    await expect(list.locator('li')).toHaveCount(2);
  });
});
