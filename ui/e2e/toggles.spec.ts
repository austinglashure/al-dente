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
