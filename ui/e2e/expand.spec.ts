import { expect, test, type Page } from '@playwright/test';
import { startServer, type Server } from './server';

/** The node-list entry for a node ID. */
const listed = (page: Page, id: string) => page.locator(`[data-testid="node-list"] [data-node-id="${id}"]`);

type Positions = Record<string, { x: number; y: number }>;

/** Every node's position on the canvas, once two reads 300 ms apart agree
 *  (the layout worker has finished).
 *  Example: `await settled(page)` is `{ 'clj:.//shop.core': { x: 1.2, y: -3 }, … }`. */
async function settled(page: Page): Promise<Positions> {
  const read = () =>
    page.evaluate(() => {
      const g = (window as unknown as { aldenteCanvas: { forEachNode: (f: (id: string, a: { x: number; y: number }) => void) => void } }).aldenteCanvas;
      const out: Record<string, { x: number; y: number }> = {};
      g.forEachNode((id, a) => {
        out[id] = { x: a.x, y: a.y };
      });
      return out;
    });
  let last = await read();
  for (let i = 0; i < 50; i += 1) {
    await page.waitForTimeout(300);
    const now = await read();
    if (JSON.stringify(now) === JSON.stringify(last)) return now;
    last = now;
  }
  throw new Error('the layout did not settle');
}

test.describe('expand in place (WP5.2)', () => {
  let server: Server;
  test.beforeAll(async () => {
    server = await startServer('clj-mini');
  });
  test.afterAll(() => server.close());

  test('expanding keeps every other node where it was; collapsing restores the aggregate and its edges', async ({ page }) => {
    await page.goto(server.url);
    await expect(listed(page, 'clj:.//shop.pricing')).toBeVisible();
    const before = await settled(page);
    const edgesBefore = await page.getByTestId('canvas').getAttribute('data-visible-edges');

    await listed(page, 'clj:.//shop.pricing').click();
    const details = page.getByTestId('details');
    await details.locator('[data-action="expand"]').click();
    await expect(listed(page, 'clj:.//shop.pricing/half-off')).toBeVisible();
    // Fully loaded: the namespace itself is folded away while expanded.
    await expect(listed(page, 'clj:.//shop.pricing')).toHaveCount(0);
    const expanded = await settled(page);
    for (const [id, p] of Object.entries(before)) {
      if (id === 'clj:.//shop.pricing') continue;
      expect(expanded[id], id).toEqual(p);
    }

    await details.locator('[data-action="collapse"]').click();
    await expect(listed(page, 'clj:.//shop.pricing/half-off')).toHaveCount(0);
    await expect(listed(page, 'clj:.//shop.pricing')).toBeVisible();
    expect(await settled(page)).toEqual(before);
    await expect(page.getByTestId('canvas')).toHaveAttribute('data-visible-edges', edgesBefore ?? '');
  });

  test('zooming out raises the semantic zoom level', async ({ page }) => {
    await page.goto(server.url);
    await expect(listed(page, 'clj:.//shop.pricing')).toBeVisible();
    const canvas = page.getByTestId('canvas');
    await expect(canvas).toHaveAttribute('data-zoom-level', '0');
    const box = await canvas.boundingBox();
    if (!box) throw new Error('no canvas');
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    for (let i = 0; i < 15; i += 1) {
      await page.mouse.wheel(0, 400);
      await page.waitForTimeout(60);
    }
    await expect(canvas).toHaveAttribute('data-zoom-level', '2');
  });
});
