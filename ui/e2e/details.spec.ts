import { expect, test, type Page } from '@playwright/test';
import { startServer, type Server } from './server';

const listed = (page: Page, id: string) => page.locator(`[data-testid="node-list"] [data-node-id="${id}"]`);

test.describe('details, source and links (WP5.4)', () => {
  let server: Server;
  test.beforeAll(async () => {
    server = await startServer('clj-mini');
  });
  test.afterAll(() => server.close());

  test('navigate caller to caller using only the keyboard', async ({ page }) => {
    await page.goto(server.url);
    await expect(listed(page, 'clj:.//shop.core')).toBeVisible();
    await page.keyboard.press('/');
    await expect(page.getByTestId('search')).toBeFocused();
    await page.keyboard.type('half-off');
    await page.keyboard.press('Enter');
    await expect(page.locator('[data-testid="results"] [data-node-id="clj:.//shop.pricing/half-off"]')).toBeVisible();
    const target = page.locator('[data-testid="results"] [data-node-id="clj:.//shop.pricing/half-off"]');
    for (let i = 0; i < 5 && !(await target.evaluate((el) => el === document.activeElement)); i += 1) await page.keyboard.press('Tab');
    await expect(target).toBeFocused();
    await page.keyboard.press('Enter');
    const details = page.getByTestId('details');
    await expect(details.locator('h2')).toHaveText('half-off');
    for (let step = 0; step < 2; step += 1) {
      const caller = details.locator('[data-testid="caller"]').first();
      await expect(caller).toBeVisible();
      const name = await caller.innerText();
      await page.keyboard.press('ArrowLeft');
      await expect(details.locator('h2')).toHaveText(name);
    }
    // And back down to a callee.
    const callee = details.locator('[data-testid="callee"]').first();
    const calleeName = await callee.innerText();
    await page.keyboard.press('ArrowRight');
    await expect(details.locator('h2')).toHaveText(calleeName);
  });

  test('source has line numbers; a call site shows its line in context', async ({ page }) => {
    await page.goto(server.url);
    await page.getByTestId('search').fill('checkout');
    await page.getByTestId('search').press('Enter');
    await page.locator('[data-testid="results"] [data-node-id="clj:.//shop.core/checkout"]').click();
    const source = page.getByTestId('source');
    await expect(source.locator('[data-line="12"]')).toContainText('(defn checkout');
    await expect(source.locator('.tok-bracket').first()).toBeVisible();
    const details = page.getByTestId('details');
    const sites = details.locator('details.sites').first();
    await sites.locator('summary').click();
    const site = sites.locator('[data-testid="site"]').first();
    const [, line] = /:(\d+):\d+$/.exec(await site.innerText()) ?? [];
    await site.click();
    await expect(page.getByTestId('site-source').locator('.line.marked')).toHaveAttribute('data-line', line ?? '');
  });

  test('open in editor without an editor configured copies the location', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    await page.goto(server.url);
    await page.getByTestId('search').fill('checkout');
    await page.getByTestId('search').press('Enter');
    await page.locator('[data-testid="results"] [data-node-id="clj:.//shop.core/checkout"]').click();
    await page.getByTestId('details').locator('[data-action="open-editor"]').click();
    await expect(page.getByTestId('editor-note')).toContainText('copied src/shop/core.clj:12:1');
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('src/shop/core.clj:12:1');
  });
});
