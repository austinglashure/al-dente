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
/** The directory-tree row for a node ID (a directory or a file's namespace). */
const inTree = (page: Page, id: string) => page.locator(`[data-testid="tree"] [data-node-id="${id}"]`);

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

  test('Help opens the measures guide with diagrams, and closes', async ({ page }) => {
    guardNetwork(page);
    await page.goto(server.url);
    await page.getByTestId('help').click();
    const dialog = page.getByTestId('help-dialog');
    await expect(dialog).toBeVisible();
    await expect(dialog.locator('[data-guide-entry="pagerank"]')).toContainText('What it tells you');
    await expect(dialog.locator('[data-guide-entry="dominates"] svg')).toBeVisible();
    // Thresholds come from the server's rules (rules/hotspots.toml), not the UI.
    await expect(dialog.locator('[data-guide-entry="risky-untested"]')).toContainText('Risk in the top 5%');
    await expect(dialog.locator('[data-guide-entry="indirect"]')).toContainText('within 4 hops');
    await page.screenshot({ path: 'test-results/help-dialog.png' });
    await page.getByTestId('help-close').click();
    await expect(dialog).toHaveCount(0);
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

test.describe('interop', () => {
  test('the interop exits toggle shows and hides platform nodes', async ({ page }) => {
    const server = await startServer('clj-mini');
    try {
      guardNetwork(page);
      await page.goto(server.url);
      await page.locator('[data-open="clj:.//shop.interop"]').click();
      const exit = listed(page, 'java:java.lang.System#currentTimeMillis');
      await expect(exit).toBeVisible();
      await page.getByTestId('hide-interop').check();
      await expect(exit).toHaveCount(0);
      await page.getByTestId('hide-interop').uncheck();
      await expect(exit).toBeVisible();
    } finally {
      server.close();
    }
  });
});

test.describe('global namespace map', () => {
  test('one click switches from units to every namespace and back', async ({ page }) => {
    const server = await startServer('mono-mini');
    try {
      const requests = guardNetwork(page);
      await page.goto(server.url);
      await expect(listed(page, 'unit:services/orders')).toBeVisible();
      await expect(page.getByTestId('view-units')).toHaveAttribute('aria-pressed', 'true');

      await page.getByTestId('view-namespaces').click();
      await expect(page.getByTestId('view-namespaces')).toHaveAttribute('aria-pressed', 'true');
      await expect(page.getByTestId('breadcrumb')).toContainText('All namespaces');
      // The left panel is the directory tree, fully expanded.
      await expect(inTree(page, 'clj:services/orders//orders.core')).toBeVisible();
      await expect(inTree(page, 'clj:services/billing//billing.core')).toBeVisible();
      await expect(page.getByTestId('node-list')).toHaveCount(0);

      // Node cap: keep the namespace level, show only the busiest.
      await page.getByTestId('top').selectOption('100');
      await expect(page.getByTestId('top')).toHaveValue('100');
      await expect.poll(() => requests.some((u) => u.includes('/tree/graph') && u.includes('top=100'))).toBe(true);

      await page.getByTestId('view-units').click();
      await expect(listed(page, 'unit:services/orders')).toBeVisible();
      await expect(page.getByTestId('top')).toHaveCount(0);
    } finally {
      server.close();
    }
  });
});

test.describe('requests', () => {
  test('the first view and a view switch each load once; aggregates ask for no source', async ({ page }) => {
    const server = await startServer('mono-mini');
    try {
      const requests = guardNetwork(page);
      const api = (path: string) => requests.filter((u) => new URL(u).pathname === `/api/v1/${path}`);
      await page.goto(server.url);
      await expect(listed(page, 'unit:services/orders')).toBeVisible();
      await page.waitForTimeout(300);
      expect(api('snapshot')).toHaveLength(1);
      expect(api('blindspots')).toHaveLength(1);
      expect(api('graph')).toHaveLength(1);

      await page.getByTestId('view-namespaces').click();
      await expect(inTree(page, 'clj:services/billing//billing.core')).toBeVisible();
      await page.getByTestId('top').selectOption('100');
      await page.waitForTimeout(300);
      // The units load, then the tree map at 250 and at 100; nothing else.
      expect(api('graph')).toHaveLength(1);
      expect(api('tree/graph')).toHaveLength(2);
      expect(api('tree')).toHaveLength(1);
      expect(api('snapshot')).toHaveLength(1);

      await inTree(page, 'clj:services/billing//billing.core').click();
      await expect(page.getByTestId('details')).toContainText('billing.core');
      await page.waitForTimeout(300);
      expect(api('source')).toHaveLength(0);
    } finally {
      server.close();
    }
  });
});

test.describe('slow reports wait for the first view', () => {
  test('hotspots and coverage start after the graph arrives; measures only when their panel opens', async ({ page }) => {
    const server = await startServer('mono-mini');
    try {
      const order: string[] = [];
      page.on('request', (r) => order.push(`start ${new URL(r.url()).pathname}`));
      page.on('response', (r) => order.push(`done ${new URL(r.url()).pathname}`));
      await page.goto(server.url);
      await expect(listed(page, 'unit:services/orders')).toBeVisible();
      await page.waitForTimeout(500);
      const at = (e: string) => order.indexOf(e);
      expect(at('done /api/v1/graph'), order.join('\n')).toBeGreaterThanOrEqual(0);
      for (const slow of ['/api/v1/hotspots', '/api/v1/coverage']) {
        expect(at(`start ${slow}`), order.join('\n')).toBeGreaterThan(at('done /api/v1/graph'));
      }
      expect(order.filter((e) => e.startsWith('start /api/v1/metrics'))).toHaveLength(0);
      await page.getByTestId('measures-panel').locator('summary').click();
      await expect.poll(() => order.filter((e) => e.startsWith('start /api/v1/metrics')).length).toBe(1);
    } finally {
      server.close();
    }
  });
});

test.describe('remembered view', () => {
  test('a reload reopens the namespace map with the chosen Top', async ({ page }) => {
    const server = await startServer('mono-mini');
    try {
      const requests = guardNetwork(page);
      await page.goto(server.url);
      await expect(listed(page, 'unit:services/orders')).toBeVisible();
      await page.getByTestId('view-namespaces').click();
      await page.getByTestId('top').selectOption('500');
      await expect(inTree(page, 'clj:services/billing//billing.core')).toBeVisible();

      const before = requests.length;
      await page.reload();
      await expect(page.getByTestId('view-namespaces')).toHaveAttribute('aria-pressed', 'true');
      await expect(page.getByTestId('top')).toHaveValue('500');
      await expect(inTree(page, 'clj:services/billing//billing.core')).toBeVisible();
      await page.waitForTimeout(300);
      const after = requests.slice(before).filter((u) => new URL(u).pathname === '/api/v1/tree/graph');
      expect(after, 'the remembered view loads once').toHaveLength(1);
    } finally {
      server.close();
    }
  });
});

test.describe('source', () => {
  test('the copy button puts the source on the clipboard', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const server = await startServer('clj-mini');
    try {
      guardNetwork(page);
      await page.goto(server.url);
      await page.locator('[data-open="clj:.//shop.core"]').click();
      await listed(page, 'clj:.//shop.core/checkout').click();
      await expect(page.getByTestId('source')).toContainText('(defn checkout');
      await page.getByTestId('copy-source').click();
      await expect(page.getByTestId('copy-source')).toHaveText('Copied');
      const copied = await page.evaluate(() => navigator.clipboard.readText());
      // The code as shown, without the line-number gutter (WP5.4).
      const code = await page
        .getByTestId('source')
        .locator('.line')
        .evaluateAll((lines) => lines.map((l) => [...l.childNodes].filter((c) => !(c instanceof HTMLElement && c.classList.contains('ln'))).map((c) => c.textContent).join('')).join(''));
      expect(copied).toBe(code);
      expect(copied).toContain('(defn checkout');
    } finally {
      server.close();
    }
  });
});

test.describe('details sections', () => {
  test('a list folds, and stays folded for the next node', async ({ page }) => {
    const server = await startServer('clj-mini');
    try {
      await page.goto(server.url);
      await listed(page, 'clj:.//shop.pricing').click();
      const details = page.getByTestId('details');
      const price = details.locator('[data-testid="child"][data-node-id="clj:.//shop.pricing/price"]');
      await expect(price).toBeVisible();
      const toggle = details.locator('[data-testid="section-toggle"][data-section="children"]');
      await toggle.click();
      await expect(price).toBeHidden();
      // Another namespace: its contents start folded too.
      await listed(page, 'clj:.//shop.core').click();
      await expect(details.locator('h2')).toHaveText('shop.core');
      await expect(details.locator('[data-testid="child"]').first()).toBeHidden();
      // And after a reload.
      await page.reload();
      await listed(page, 'clj:.//shop.pricing').click();
      await expect(price).toBeHidden();
      await toggle.click();
      await expect(price).toBeVisible();
    } finally {
      server.close();
    }
  });
});

test.describe('directory tree', () => {
  test('open a directory: only its namespaces and the calls between them', async ({ page }) => {
    const server = await startServer('mono-mini');
    try {
      await page.goto(server.url);
      await page.getByTestId('view-namespaces').click();
      const services = page.locator('[data-testid="tree-toggle"][data-dir="dir:services"]');
      await services.click();
      await expect(services).toHaveAttribute('aria-expanded', 'false');
      await inTree(page, 'dir:services').click();
      const details = page.getByTestId('details');
      await details.locator('[data-action="keep-only"]').click();
      await details.locator('[data-action="open"]').click();
      await expect(page.getByTestId('breadcrumb')).toContainText('services/');
      await expect(listed(page, 'clj:services/orders//orders.core')).toBeVisible();
      await expect(listed(page, 'clj:services/billing//billing.core')).toBeVisible();
      // Nothing from outside the directory, and no directory nodes.
      await expect(listed(page, 'clj:libs/common//common.util')).toHaveCount(0);
      await expect(page.locator('[data-testid="node-list"] [data-node-id^="dir:"]')).toHaveCount(0);
    } finally {
      server.close();
    }
  });

  test('collapsing a directory folds the graph to it; expand all and collapse all', async ({ page }) => {
    const server = await startServer('mono-mini');
    try {
      const requests = guardNetwork(page);
      await page.goto(server.url);
      await page.getByTestId('view-namespaces').click();
      const services = page.locator('[data-testid="tree-toggle"][data-dir="dir:services"]');
      await expect(services).toHaveAttribute('aria-expanded', 'true');

      await services.click();
      await expect(services).toHaveAttribute('aria-expanded', 'false');
      await expect(inTree(page, 'clj:services/orders//orders.core')).toHaveCount(0);
      await expect.poll(() => requests.some((u) => u.includes('/tree/graph') && decodeURIComponent(u).includes('collapsed=dir:services'))).toBe(true);

      // A directory selected shows what it holds and what it uses.
      await inTree(page, 'dir:services').click();
      const details = page.getByTestId('details');
      await expect(details.locator('h2')).toHaveText('services/');
      await expect(details).toContainText('directory');
      await expect(details.locator('[data-testid="child"][data-node-id="dir:services/orders"]')).toBeVisible();
      // Two rows: a calls edge and a requires edge.
      await expect(details.locator('[data-testid="callee"][data-node-id="clj:libs/common//common.util"]')).toHaveCount(2);

      // Double-clicking a collapsed directory on the canvas expands it (via the tools here).
      await details.locator('[data-action="unfold"]').click();
      await expect(services).toHaveAttribute('aria-expanded', 'true');
      await expect(inTree(page, 'clj:services/orders//orders.core')).toBeVisible();

      await page.getByTestId('tree-collapse-all').click();
      await expect(services).toHaveAttribute('aria-expanded', 'false');
      await expect(page.locator('[data-testid="tree-toggle"][data-dir="dir:libs/common/src/common"]')).toHaveAttribute('aria-expanded', 'false');
      await page.getByTestId('tree-expand-all').click();
      await expect(services).toHaveAttribute('aria-expanded', 'true');
      await expect(inTree(page, 'clj:libs/common//common.util')).toBeVisible();
    } finally {
      server.close();
    }
  });
});
