// WP5.2 acceptance on the synthetic corpus (20 units, 8,000 namespaces,
// 300,000 symbols, 2,000,000 edges). Opt-in: it needs release builds and
// about a minute to generate the corpus the first time.
//
//   cargo build --release -p aldente-cli -p bench
//   ALDENTE_E2E_LARGE=1 ALDENTE_BIN=../target/release/aldente npx playwright test e2e/large.spec.ts
//
// ALDENTE_E2E_RELAXED=<factor> multiplies the time budgets and divides the
// frame-rate floor (for CI hardware; say so wherever the result is quoted).
// Headless Chromium draws WebGL in software (SwiftShader) unless told to
// use the GPU: ALDENTE_E2E_GPU=1 does (ANGLE on OpenGL ES, the integrated
// GPU on a typical laptop). The 30 fps floor is checked only with it.

import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';
import { repoFile, startSnapshotServer, type Server } from './server';

const enabled = process.env.ALDENTE_E2E_LARGE === '1';
const relax = Number(process.env.ALDENTE_E2E_RELAXED ?? '1');
const corpus = repoFile('target/bench/synthetic.db');

/** The node-list entry for a node ID. */
const listed = (page: Page, id: string) => page.locator(`[data-testid="node-list"] [data-node-id="${id}"]`);

/** Frames the page draws per second while the camera zooms and pans.
 *  Counts `requestAnimationFrame` callbacks: a slow redraw blocks the main
 *  thread and lowers the count. */
async function framesPerSecond(page: Page): Promise<number> {
  const box = await page.getByTestId('canvas').boundingBox();
  if (!box) throw new Error('no canvas');
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  await page.evaluate(() => {
    const w = window as unknown as { frames: number; counting: boolean };
    w.frames = 0;
    w.counting = true;
    const tick = () => {
      if (!w.counting) return;
      w.frames += 1;
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });
  const started = Date.now();
  await page.mouse.move(cx, cy);
  let i = 0;
  while (Date.now() - started < 3000) {
    await page.mouse.wheel(0, i % 20 < 10 ? 120 : -120);
    await page.mouse.down();
    await page.mouse.move(cx + ((i % 10) - 5) * 8, cy + ((i % 7) - 3) * 8);
    await page.mouse.up();
    i += 1;
  }
  const frames = await page.evaluate(() => {
    const w = window as unknown as { frames: number; counting: boolean };
    w.counting = false;
    return w.frames;
  });
  return frames / ((Date.now() - started) / 1000);
}

const useGpu = process.env.ALDENTE_E2E_GPU === '1';
const gpu = useGpu ? ['--ignore-gpu-blocklist', '--enable-gpu', '--use-angle=gl-egl'] : [];
test.use({ launchOptions: { args: gpu } });

test.describe('large synthetic corpus (WP5.2)', () => {
  test.skip(!enabled, 'set ALDENTE_E2E_LARGE=1 (see the header)');
  let server: Server;
  test.beforeAll(async () => {
    test.setTimeout(300_000);
    if (!existsSync(corpus)) {
      execFileSync(repoFile('target/release/bench'), ['gen', '--out', corpus], { stdio: 'inherit' });
    }
    server = await startSnapshotServer(corpus);
  });
  test.afterAll(() => server?.close());

  test('initial view, a 5,000-symbol namespace in pages, and 30 frames a second', async ({ page }) => {
    test.setTimeout(600_000);
    const started = Date.now();
    await page.goto(server.url);
    await expect(listed(page, 'unit:services/svc-00')).toBeVisible({ timeout: 30_000 });
    const initial = Date.now() - started;
    console.log(`initial view: ${initial} ms`);
    expect(initial).toBeLessThan(2000 * relax);

    const details = page.getByTestId('details');
    await listed(page, 'unit:services/svc-00').click();
    await details.locator('[data-action="expand"]').click();
    const big = 'clj:services/svc-00//synth.u00.n0000';
    await expect(listed(page, big)).toBeVisible({ timeout: 30_000 });
    await listed(page, big).click();
    await expect(details.locator('h2')).toHaveText('synth.u00.n0000');

    let pages = 0;
    const times: number[] = [];
    let action = details.locator('[data-action="expand"]');
    for (;;) {
      const nodes = Number(await page.getByTestId('canvas').getAttribute('data-nodes'));
      const t = Date.now();
      await action.click();
      await expect(page.getByTestId('canvas')).not.toHaveAttribute('data-nodes', String(nodes), { timeout: 30_000 });
      times.push(Date.now() - t);
      pages += 1;
      console.log(`page ${pages}: ${times[times.length - 1]} ms, ${await page.getByTestId('canvas').getAttribute('data-nodes')} nodes`);
      action = details.locator('[data-action="show-more"]');
      if ((await action.count()) === 0) break;
    }
    console.log(`pages: ${pages}, ms each: ${times.join(' ')}`);
    expect(pages).toBe(25);

    // Let the last layout land, then measure with everything on screen.
    await page.waitForTimeout(5000);
    const canvas = page.getByTestId('canvas');
    const nodes = Number(await canvas.getAttribute('data-nodes'));
    const edges = Number(await canvas.getAttribute('data-visible-edges'));
    console.log(`on screen: ${nodes} nodes, ${edges} edges`);
    expect(nodes).toBeGreaterThanOrEqual(5000);
    const fps = await framesPerSecond(page);
    console.log(`frames per second while zooming and panning: ${fps.toFixed(1)}${useGpu ? "" : " (software WebGL, floor not checked)"}`);
    // The 30 fps floor is about GPU drawing. In software WebGL (CI, or local
    // runs without ALDENTE_E2E_GPU=1) the rate measures the CPU, so only
    // check that the page keeps drawing while the camera moves.
    if (useGpu) expect(fps).toBeGreaterThan(30 / relax);
    else expect(fps).toBeGreaterThan(0);
  });
});
