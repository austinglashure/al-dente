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
    await expect(page.getByTestId('notice')).toHaveCount(0);

    await page.locator('[data-open="clj:.//shop.core"]').click();
    await expect(listed(page, 'clj:.//shop.core/checkout')).toBeVisible();

    await listed(page, 'clj:.//shop.core/checkout').click();
    const details = page.getByTestId('details');
    await expect(details).toContainText('checkout');
    await expect(details).toContainText('src/shop/core.clj:12:1');
    await expect(page.getByTestId('source')).toContainText('(defn checkout');

    await details.locator('[data-testid="callee"][data-node-id="clj:.//shop.pricing/price"]').click();
    await expect(details.locator('h2')).toHaveText('price');
    await expect(page.getByTestId('source')).toContainText('(defmulti price');
    await expect(details.locator('[data-action="focus"]')).toBeVisible();

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

test.describe('opening units and namespaces', () => {
  test('a namespace shows its contents and opens on its own canvas', async ({ page }) => {
    const server = await startServer('clj-mini');
    try {
      guardNetwork(page);
      await page.goto(server.url);
      await expect(page.getByTestId('hint')).toBeVisible();
      await listed(page, 'clj:.//shop.pricing').click();
      const details = page.getByTestId('details');
      await expect(details).toContainText('Contains (2)');
      await expect(details.locator('[data-testid="child"][data-node-id="clj:.//shop.pricing/price"]')).toBeVisible();
      await expect(details).toContainText('shop.util');
      await details.locator('[data-action="open"]').click();
      await expect(page.getByTestId('breadcrumb')).toContainText('shop.pricing');
      await expect(listed(page, 'clj:.//shop.pricing/half-off')).toBeVisible();
      // Namespaces that talk to it stay as context; unrelated ones go.
      await expect(listed(page, 'clj:.//shop.core')).toBeVisible();
      await expect(listed(page, 'clj:.//shop.audit')).toHaveCount(0);
      await page.getByTestId('breadcrumb').getByRole('button', { name: 'Repository' }).click();
      await expect(listed(page, 'clj:.//shop.core')).toBeVisible();
    } finally {
      server.close();
    }
  });

  test('a unit shows its build files and opens to its namespaces', async ({ page }) => {
    const server = await startServer('mono-mini');
    try {
      guardNetwork(page);
      await page.goto(server.url);
      await expect(listed(page, 'unit:services/orders')).toHaveText('orders');
      await listed(page, 'unit:services/orders').click();
      const details = page.getByTestId('details');
      await expect(details).toContainText('deps.edn');
      await expect(details).toContainText('services/orders/test');
      await expect(details).toContainText('common');
      await details.locator('[data-action="open"]').click();
      await expect(page.getByTestId('breadcrumb')).toContainText('orders');
      await expect(listed(page, 'clj:services/orders//orders.core')).toBeVisible();
      await expect(listed(page, 'clj:services/billing//billing.core')).toHaveCount(0);
    } finally {
      server.close();
    }
  });
});

test.describe('panels', () => {
  let server: Server;
  test.beforeAll(async () => {
    server = await startServer('clj-mini');
  });
  test.afterAll(() => server.close());

  test('Escape and the close button deselect', async ({ page }) => {
    guardNetwork(page);
    await page.goto(server.url);
    await listed(page, 'clj:.//shop.core').click();
    await expect(page.getByTestId('details')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.getByTestId('hint')).toBeVisible();
    await expect(page.getByTestId('details')).toHaveCount(0);
    await listed(page, 'clj:.//shop.core').click();
    await page.locator('[data-action="close"]').click();
    await expect(page.getByTestId('hint')).toBeVisible();
  });

  test('panels can be hidden and resized, and remember it', async ({ page }) => {
    guardNetwork(page);
    await page.goto(server.url);
    await expect(page.getByTestId('node-list')).toBeVisible();
    await page.getByTestId('toggle-left').click();
    await expect(page.getByTestId('node-list')).toBeHidden();
    await page.getByTestId('toggle-left').click();
    await expect(page.getByTestId('node-list')).toBeVisible();
    await page.getByTestId('toggle-right').click();
    await expect(page.getByTestId('hint')).toBeHidden();
    await page.getByTestId('toggle-right').click();

    const right = page.locator('aside.right');
    const before = (await right.boundingBox())?.width ?? 0;
    const handle = page.getByTestId('resize-right');
    const box = await handle.boundingBox();
    if (!box) throw new Error('no resize handle');
    await page.mouse.move(box.x + box.width / 2, box.y + 100);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 - 120, box.y + 100, { steps: 5 });
    await page.mouse.up();
    const after = (await right.boundingBox())?.width ?? 0;
    expect(after).toBeGreaterThan(before + 80);

    await page.reload();
    const reloaded = (await page.locator('aside.right').boundingBox())?.width ?? 0;
    expect(Math.abs(reloaded - after)).toBeLessThan(5);
  });
});

test.describe('certainty', () => {
  test('toggling possible off hides the possible edges', async ({ page }) => {
    const server = await startServer('clj-mini');
    try {
      guardNetwork(page);
      await page.goto(server.url);
      await page.locator('[data-open="clj:.//shop.util"]').click();
      await expect(listed(page, 'clj:.//shop.util/with-timing')).toBeVisible();
      const possible = page.getByTestId('legend-possible');
      await expect(possible).toContainText(/possible \([1-9]/);
      const canvas = page.getByTestId('canvas');
      const before = Number(await canvas.getAttribute('data-visible-edges'));
      const count = Number((await possible.textContent())?.match(/\((\d+)\)/)?.[1]);
      await page.getByTestId('show-possible').uncheck();
      await expect(canvas).toHaveAttribute('data-visible-edges', String(before - count));
      await page.getByTestId('show-possible').check();
      await expect(canvas).toHaveAttribute('data-visible-edges', String(before));
    } finally {
      server.close();
    }
  });
});

test.describe('blind spots', () => {
  test('holes are listed and link to the graph', async ({ page }) => {
    const server = await startServer('clj-mini');
    try {
      guardNetwork(page);
      await page.goto(server.url);
      const list = page.getByTestId('blindspots');
      await expect(list).toContainText('dynamic-resolve');
      await expect(list).toContainText('dynamic-eval');
      await expect(list).toContainText('macro-generated');
      await list.locator('[data-node-id="hole:dynamic-eval@src/shop/plugins.clj:12:3"]').click();
      const details = page.getByTestId('details');
      await expect(details).toContainText('hole');
      await expect(details).toContainText('run-script');
    } finally {
      server.close();
    }
  });
});
