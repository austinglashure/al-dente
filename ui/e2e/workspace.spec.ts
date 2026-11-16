import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';
import { startServer, startServerOn, type Server } from './server';

const listed = (page: Page, id: string) => page.locator(`[data-testid="node-list"] [data-node-id="${id}"]`);

/** What the canvas shows: the listed node IDs and every node's rounded
 *  position, once the layout has settled. */
async function view(page: Page): Promise<{ listed: string[]; positions: Record<string, [number, number]> }> {
  const read = () =>
    page.evaluate(() => {
      const g = (window as unknown as { aldenteCanvas: { forEachNode: (f: (id: string, a: { x: number; y: number }) => void) => void } }).aldenteCanvas;
      const out: Record<string, [number, number]> = {};
      g.forEachNode((id, a) => {
        out[id] = [Math.round(a.x) + 0, Math.round(a.y) + 0];
      });
      return out;
    });
  await expect(page.getByTestId('canvas')).toHaveAttribute('data-layout-pending', '0', { timeout: 30_000 });
  const last = await read();
  const ids = await page.locator('[data-testid="node-list"] [data-node-id]').evaluateAll((els) => els.map((e) => e.getAttribute('data-node-id') ?? ''));
  return { listed: ids.sort(), positions: last };
}

test.describe('pruning workspace (WP5.3)', () => {
  let server: Server;
  test.beforeAll(async () => {
    server = await startServer('mono-mini');
  });
  test.afterAll(() => server.close());

  test('six steps, undo to the start, redo, and a reload from the URL give the same view', async ({ page }) => {
    await page.goto(server.url);
    await expect(listed(page, 'unit:services/orders')).toBeVisible();
    const details = page.getByTestId('details');
    const steps = [await view(page)];
    const select = async (id: string) => {
      await listed(page, id).click();
      await expect(details.locator('h2')).toBeVisible();
    };
    // 1–2: expand two units in place.
    await select('unit:services/orders');
    await details.locator('[data-action="expand"]').click();
    await expect(listed(page, 'clj:services/orders//orders.core')).toBeVisible();
    steps.push(await view(page));
    await select('unit:services/billing');
    await details.locator('[data-action="expand"]').click();
    await expect(listed(page, 'clj:services/billing//billing.core')).toBeVisible();
    steps.push(await view(page));
    // 3: pin a namespace.
    await select('clj:services/orders//orders.core');
    await details.locator('[data-action="pin"]').click();
    steps.push(await view(page));
    // 4: hide the other one.
    await select('clj:services/billing//billing.core');
    await details.locator('[data-action="hide"]').click();
    await expect(listed(page, 'clj:services/billing//billing.core')).toHaveCount(0);
    steps.push(await view(page));
    // 5: a name filter.
    await page.getByTestId('filters').locator('summary').click();
    await page.getByTestId('filter-name').fill('orders');
    await page.getByTestId('filter-name').press('Enter');
    await expect(listed(page, 'unit:services/billing')).toHaveCount(0);
    steps.push(await view(page));
    // 6: hide tests.
    await page.getByTestId('hide-tests').check();
    steps.push(await view(page));
    const last = steps[steps.length - 1];
    expect(new Set(steps.map((s) => JSON.stringify(s.listed))).size, 'every step changed what is listed or where').toBeGreaterThan(3);

    for (let i = steps.length - 2; i >= 0; i -= 1) {
      await page.getByTestId('undo').click();
      expect(await view(page), `after undoing to step ${i}`).toEqual(steps[i]);
    }
    await expect(page.getByTestId('undo')).toBeDisabled();
    for (let i = 1; i < steps.length; i += 1) {
      await page.getByTestId('redo').click();
      expect(await view(page), `after redoing step ${i}`).toEqual(steps[i]);
    }
    await expect(page.getByTestId('redo')).toBeDisabled();

    await expect(page).toHaveURL(/#w=/);
    await page.reload();
    await expect(listed(page, 'clj:services/orders//orders.core')).toBeVisible();
    expect(await view(page)).toEqual(last);
    await expect(page.getByTestId('hide-tests')).toBeChecked();
    await expect(page.getByTestId('filter-name')).toHaveValue('orders');
  });

  test('box select marks nodes; Keep only removes the rest and can be undone', async ({ page }) => {
    await page.goto(server.url);
    await expect(listed(page, 'unit:services/orders')).toBeVisible();
    const before = await view(page);
    const box = await page.getByTestId('canvas').boundingBox();
    if (!box) throw new Error('no canvas');
    await page.keyboard.down('Shift');
    await page.mouse.move(box.x + 2, box.y + 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height - 2, { steps: 5 });
    await page.mouse.up();
    await page.keyboard.up('Shift');
    const bar = page.getByTestId('selection-bar');
    await expect(bar).toBeVisible();
    const marked = Number((await bar.innerText()).split(' ')[0]);
    expect(marked).toBeGreaterThan(0);
    expect(marked).toBeLessThan(before.listed.length);
    await bar.locator('[data-action="keep-marked"]').click();
    await expect(page.locator('[data-testid="node-list"] [data-node-id]')).toHaveCount(marked);
    await page.keyboard.press('Control+z');
    expect((await view(page)).listed).toEqual(before.listed);
  });

  test('a saved workspace loads by name', async ({ page }) => {
    await page.goto(server.url);
    await expect(listed(page, 'unit:services/orders')).toBeVisible();
    await listed(page, 'unit:services/orders').click();
    await page.getByTestId('details').locator('[data-action="expand"]').click();
    await expect(listed(page, 'clj:services/orders//orders.core')).toBeVisible();
    await page.getByTestId('workspace-name').fill('orders open');
    await page.getByTestId('workspace-save').click();
    await page.getByTestId('view-namespaces').click();
    await expect(listed(page, 'unit:services/orders')).toHaveCount(0);
    await page.locator('[data-workspace="orders open"]').click();
    await expect(listed(page, 'clj:services/orders//orders.core')).toBeVisible();
  });
});

test.describe('a workspace too long for a URL (WP5.3)', () => {
  let server: Server;
  let dir: string;
  test.beforeAll(async () => {
    dir = mkdtempSync(join(tmpdir(), 'aldente-e2e-wide-'));
    writeFileSync(join(dir, 'deps.edn'), '{:paths ["src"]}\n');
    mkdirSync(join(dir, 'src', 'wide'), { recursive: true });
    let src = '(ns wide.core)\n';
    for (let i = 0; i < 600; i += 1) src += `\n(defn handler-for-request-kind-${i} [x] ${i > 0 ? `(handler-for-request-kind-${i - 1} x)` : 'x'})\n`;
    writeFileSync(join(dir, 'src', 'wide', 'core.clj'), src);
    server = await startServerOn(dir);
  });
  test.afterAll(() => {
    server.close();
    rmSync(dir, { recursive: true, force: true });
  });

  test('falls back to browser storage under a short key, says so, and reloads', async ({ page }) => {
    test.setTimeout(120_000);
    await page.goto(server.url);
    await expect(listed(page, 'clj:.//wide.core')).toBeVisible();
    await listed(page, 'clj:.//wide.core').click();
    const details = page.getByTestId('details');
    await details.locator('[data-action="expand"]').click();
    const rows = page.locator('[data-testid="node-list"] [data-node-id]');
    await expect(rows).toHaveCount(201);
    for (let i = 0; i < 2; i += 1) {
      const before = await rows.count();
      await expect(details.locator('[data-action="show-more"]')).toBeVisible();
      await details.locator('[data-action="show-more"]').click();
      await expect(rows).not.toHaveCount(before);
    }
    await expect(details.locator('[data-action="show-more"]')).toHaveCount(0);
    await expect(page.getByTestId('notice')).toContainText('kept in this browser', { timeout: 10_000 });
    await expect(page).toHaveURL(/#ws=[0-9a-f]{8}$/);
    const count = await page.locator('[data-testid="node-list"] [data-node-id]').count();
    expect(count).toBeGreaterThanOrEqual(600);
    await page.reload();
    await expect(page.locator('[data-testid="node-list"] [data-node-id]')).toHaveCount(count, { timeout: 30_000 });
  });
});
