import { expect, test, type Page } from '@playwright/test';
import { startServer, type Server } from './server';

const listed = (page: Page, id: string) => page.locator(`[data-testid="node-list"] [data-node-id="${id}"]`);

test.describe('service overview (WP5.5)', () => {
  let server: Server;
  test.beforeAll(async () => {
    server = await startServer('mono-mini');
  });
  test.afterAll(() => server.close());

  test('lands on every unit and the links between them; drills into a unit and back', async ({ page }) => {
    await page.goto(server.url);
    for (const unit of ['unit:services/orders', 'unit:services/billing', 'unit:libs/common']) {
      await expect(listed(page, unit)).toBeVisible();
    }
    const units = await page.evaluate(async () => {
      const token = sessionStorage.getItem('aldente-token') ?? '';
      const r = await fetch('/api/v1/snapshot', { headers: { 'x-aldente-token': token } });
      return ((await r.json()) as { data: { units: unknown[] } }).data.units.length;
    });
    await expect(page.locator('[data-testid="node-list"] [data-node-id^="unit:"]')).toHaveCount(units);
    expect(Number(await page.getByTestId('canvas').getAttribute('data-visible-edges'))).toBeGreaterThan(0);
    await expect(page.getByTestId('network-note')).toContainText('added in the surface view');

    await listed(page, 'unit:services/orders').click();
    const uses = page.getByTestId('unit-uses');
    await expect(uses.locator('[data-node-id="unit:libs/common"]')).toBeVisible();
    await expect(uses).toContainText(':local/root');
    await expect(uses).toContainText(/uses \d+ symbols? from it/);

    await page.getByTestId('details').locator('[data-action="open"]').click();
    await expect(page.getByTestId('breadcrumb')).toContainText('orders');
    await expect(listed(page, 'clj:services/orders//orders.core')).toBeVisible();
    await expect(page.getByTestId('network-note')).toHaveCount(0);
    await page.getByTestId('breadcrumb').getByRole('button', { name: 'Repository' }).click();
    await expect(listed(page, 'unit:libs/common')).toBeVisible();
    await expect(page.getByTestId('network-note')).toBeVisible();
  });
});
