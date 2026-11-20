import { expect, test, type Page } from '@playwright/test';
import { startServer, type Server } from './server';

const listed = (page: Page, id: string) => page.locator(`[data-testid="node-list"] [data-node-id="${id}"]`);

test.describe('hide Clojure core', () => {
  let server: Server;
  test.beforeAll(async () => {
    server = await startServer('clj-mini');
  });
  test.afterAll(() => server.close());

  test('hides clojure.core and shows it again', async ({ page }) => {
    await page.goto(server.url);
    await page.locator('[data-open="clj:.//shop.core"]').click();
    await page.getByTestId('hide-externals').uncheck();
    await expect(listed(page, 'ext:clj//clojure.core')).toBeVisible();
    await page.getByTestId('hide-core').check();
    await expect(listed(page, 'ext:clj//clojure.core')).toHaveCount(0);
    // A function's neighbors list core functions one by one.
    await page.locator('[data-testid="node-list"] [data-node-id="clj:.//shop.core/checkout"]').click();
    await page.getByTestId('details').locator('[data-action="neighbors"]').click();
    await expect(listed(page, 'ext:clj//clojure.core/let')).toHaveCount(0);
    await page.getByTestId('hide-core').uncheck();
    await expect(listed(page, 'ext:clj//clojure.core/let')).toBeVisible();
    await expect(listed(page, 'ext:clj//clojure.core')).toBeVisible();
  });
});

test.describe('one filter set for every view', () => {
  let server: Server;
  test.beforeAll(async () => {
    server = await startServer('clj-mini');
  });
  test.afterAll(() => server.close());

  test('the left panel filters apply to the crater on screen', async ({ page }) => {
    await page.goto(server.url);
    // The main view hides external libraries by default; so does the crater.
    await page.getByTestId('hide-externals').uncheck();
    await page.locator('[data-open="clj:.//shop.core"]').click();
    await listed(page, 'clj:.//shop.core/checkout').click();
    await page.getByTestId('details').locator('[data-action="crater-forward"]').click();
    const view = page.getByTestId('crater-view');
    await expect(page.getByTestId('crater-summary')).toBeVisible();
    await expect(view).toHaveAttribute('data-hidden', '0');
    // No second, crater-only copy of the toggle.
    await expect(page.getByTestId('crater-hide-core')).toHaveCount(0);
    await page.getByTestId('hide-core').check();
    // checkout's forward crater reaches let, format, swap!, str, … in clojure.core and cljs.core.
    await expect.poll(async () => Number(await view.getAttribute('data-hidden'))).toBeGreaterThan(3);
    await page.getByTestId('hide-core').uncheck();
    await expect(view).toHaveAttribute('data-hidden', '0');
    // Certainty toggles hide edges in the crater too.
    const edges = Number(await view.getAttribute('data-visible-edges'));
    await page.getByTestId('show-certain').uncheck();
    await expect.poll(async () => Number(await view.getAttribute('data-visible-edges'))).toBeLessThan(edges);
  });
});
