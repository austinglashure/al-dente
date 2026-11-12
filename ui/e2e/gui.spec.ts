import { expect, test, type Page } from '@playwright/test';
import { startServer, type Server } from './server';

/** Records every request and fails any that leaves 127.0.0.1.
 *  Example: `const requests = guardNetwork(page)`. */
function guardNetwork(page: Page): string[] {
  const requests: string[] = [];
  void page.route('**/*', (route) => {
    const url = new URL(route.request().url());
    requests.push(url.href);
    if (url.hostname !== '127.0.0.1') return route.abort();
    return route.continue();
  });
  return requests;
}

/** The node-list entry for a node ID. */
const listed = (page: Page, id: string) => page.locator(`[data-testid="node-list"] [data-node-id="${id}"]`);

test.describe('clj-mini', () => {
  let server: Server;
  test.beforeAll(async () => {
    server = await startServer('clj-mini');
  });
  test.afterAll(() => server.close());

  test('load, expand a namespace, select a symbol, follow a callee, see its source', async ({ page }) => {
    const requests = guardNetwork(page);
    await page.goto(server.url);
    await expect(listed(page, 'clj:.//shop.core')).toBeVisible();
    await expect(page).not.toHaveURL(/token=/);

    await page.locator('[data-expand="clj:.//shop.core"]').click();
    await expect(listed(page, 'clj:.//shop.core/checkout')).toBeVisible();

    await listed(page, 'clj:.//shop.core/checkout').click();
    const details = page.getByTestId('details');
    await expect(details).toContainText('checkout');
    await expect(details).toContainText('src/shop/core.clj:12:1');
    await expect(page.getByTestId('source')).toContainText('(defn checkout');

    await details.locator('[data-testid="callee"][data-node-id="clj:.//shop.pricing/price"]').click();
    await expect(details.locator('h2')).toHaveText('price');
    await expect(page.getByTestId('source')).toContainText('(defmulti price');

    for (const url of requests) {
      expect(new URL(url).hostname).toBe('127.0.0.1');
    }
    expect(requests.length).toBeGreaterThan(0);
  });

  test('choosing a search result loads its neighborhood; toggles hide nodes', async ({ page }) => {
    guardNetwork(page);
    await page.goto(server.url);
    await page.getByTestId('search').fill('half-off');
    await page.getByTestId('search').press('Enter');
    await page.locator('[data-testid="results"] [data-node-id="clj:.//shop.pricing/half-off"]').click();
    await expect(listed(page, 'clj:.//shop.pricing/price@:book')).toBeVisible();
    await page.getByTestId('hide-tests').check();
    await expect(listed(page, 'clj:.//shop.pricing-test')).toHaveCount(0);
  });
});

test.describe('hostile case 4', () => {
  let server: Server;
  test.beforeAll(async () => {
    server = await startServer('hostile/xss');
  });
  test.afterAll(() => server.close());

  test('a docstring with <script> and onerror renders as text and runs nothing', async ({ page }) => {
    const requests = guardNetwork(page);
    await page.goto(server.url);
    await page.getByTestId('search').fill('evil');
    await page.getByTestId('search').press('Enter');
    await page.locator('[data-testid="results"] [data-node-id="clj:.//xss.core/evil"]').click();
    const doc = page.getByTestId('doc');
    await expect(doc).toContainText("<script>window.__pwned = 'script'</script>");
    await expect(doc).toContainText('onerror=');
    await expect(page.getByTestId('source')).toContainText('<img src=x');
    expect(await page.evaluate(() => (window as unknown as { __pwned?: string }).__pwned)).toBeUndefined();
    expect(await page.locator('[data-testid="details"] img, [data-testid="details"] script').count()).toBe(0);
    expect(requests.some((u) => new URL(u).pathname === '/x')).toBe(false);
  });
});
