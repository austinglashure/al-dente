// Milestone M5 walkthrough (docs/plan/phases/phase-05.md, WP5.6): on an
// acceptance repository, drive the real GUI through land on the overview →
// search → neighborhood → prune to one unit → crater → source → editor,
// screenshotting each step and timing it. Writes to docs/milestones/m5/.
//
// Usage (from ui/, after `cargo build --release -p aldente-cli`):
//   node scripts/m5-walkthrough.mjs            # reitit (default)
//   node scripts/m5-walkthrough.mjs penpot     # once penpot can be fetched
// ALDENTE_BIN picks the binary (default ../target/release/aldente).

import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..', '..');
const bin = process.env.ALDENTE_BIN ?? join(root, 'target', 'release', process.platform === 'win32' ? 'aldente.exe' : 'aldente');
const out = join(root, 'docs', 'milestones', 'm5');

// Per repository: what to search for, which result to pick, which unit to prune to.
const repos = {
  reitit: {
    url: 'https://github.com/metosin/reitit',
    rev: '1d2fc6735f53e2026fed98e92322a29f7a35c371',
    search: 'resolve-routes',
    pick: 'clj:modules/reitit-core//reitit.impl/resolve-routes',
    unit: 'unit:modules/reitit-core',
  },
};

/** Starts `aldente serve` with `env` and resolves with its URL and process.
 *  Example: `await serve(repos.reitit, env)` gives `{ url: 'http://127.0.0.1:…/#token=…', child }`. */
function serve(repo, env) {
  const child = spawn(bin, ['serve', '--no-open', '--json', '--ref', repo.rev, repo.url], { env, stdio: ['ignore', 'pipe', 'inherit'] });
  return new Promise((ok, fail) => {
    let buf = '';
    child.stdout.on('data', (c) => {
      buf += c;
      const end = buf.indexOf('\n}\n');
      if (end >= 0) ok({ url: JSON.parse(buf.slice(0, end + 2)).data.url, child });
    });
    child.on('exit', (code) => fail(new Error(`serve exited ${code}`)));
  });
}

/** Runs one walkthrough step: `act`, then a screenshot named `<n>-<name>.png`;
 *  records the step's wall time (action until its awaited result is visible).
 *  Example: `await step(1, 'overview', 'Land on the overview', async () => …)`. */
async function step(page, steps, n, name, title, act) {
  const start = performance.now();
  const note = await act();
  const ms = Math.round(performance.now() - start);
  await page.waitForTimeout(1500); // let the layout settle for the picture; not timed
  const shot = `${repoName}-${n}-${name}.png`;
  await page.screenshot({ path: join(out, shot) });
  steps.push({ step: n, title, ms, screenshot: shot, ...(note ? { note } : {}) });
  console.log(`${n}. ${title}: ${ms} ms${note ? ` (${note})` : ''}`);
}

const repoName = process.argv[2] ?? 'reitit';
const repo = repos[repoName];
if (!repo) {
  console.error(`unknown repository ${repoName}; known: ${Object.keys(repos).join(', ')}`);
  process.exit(2);
}
mkdirSync(out, { recursive: true });

// A config dir of our own whose editor is the recorder, so step 7 needs no real editor.
const scratch = mkdtempSync(join(tmpdir(), 'aldente-m5-'));
const editorLog = join(scratch, 'editor.json');
const recorder = join(here, 'record-editor.mjs').replaceAll('\\', '/');
writeFileSync(join(scratch, 'config.toml'), `editor = [${JSON.stringify(process.execPath.replaceAll('\\', '/'))}, ${JSON.stringify(recorder)}, "{file}", "{line}", "{col}"]\n`);
const env = { ...process.env, ALDENTE_CONFIG_DIR: scratch, ALDENTE_M5_EDITOR_LOG: editorLog };

const { url, child } = await serve(repo, env);
const browser = await chromium.launch({ channel: 'chromium' });
const steps = [];
let location = ''; // `path:line:col` of the picked symbol, set in step 6
try {
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
  // Nothing leaves the machine: refuse every request not to the local server.
  await page.route('**/*', (r) => (new URL(r.request().url()).hostname === '127.0.0.1' ? r.continue() : r.abort()));
  const details = page.getByTestId('details');

  await step(page, steps, 1, 'overview', 'Land on the overview', async () => {
    await page.goto(url);
    await page.locator('[data-testid="node-list"] [data-node-id^="unit:"]').first().waitFor();
    return `${await page.locator('[data-testid="node-list"] [data-node-id^="unit:"]').count()} units`;
  });

  await step(page, steps, 2, 'search', `Find ${repo.search} by search`, async () => {
    await page.keyboard.press('/');
    await page.keyboard.type(repo.search);
    await page.keyboard.press('Enter');
    await page.locator(`[data-testid="results"] [data-node-id="${repo.pick}"]`).waitFor();
    return `${await page.locator('[data-testid="results"] [data-node-id]').count()} results`;
  });

  await step(page, steps, 3, 'neighborhood', 'See its neighborhood', async () => {
    await page.locator(`[data-testid="results"] [data-node-id="${repo.pick}"]`).click();
    await details.locator('h2').filter({ hasText: repo.search }).waitFor();
    await page.locator(`[data-testid="node-list"] [data-node-id="${repo.pick}"]`).waitFor();
    return `${await page.locator('[data-testid="node-list"] [data-node-id]').count()} nodes listed`;
  });

  await step(page, steps, 4, 'prune', `Prune to one unit (${repo.unit.replace(/^unit:/, '')})`, async () => {
    const before = await page.locator('[data-testid="node-list"] [data-node-id]').count();
    await page.getByTestId('filters').locator('summary').click();
    await page.locator(`[data-testid="filter-units"] input[data-value="${repo.unit}"]`).check();
    await page.getByTestId('filters').locator('summary', { hasText: '(on)' }).waitFor();
    const after = await page.locator('[data-testid="node-list"] [data-node-id]').count();
    return `${before} → ${after} nodes listed`;
  });

  await step(page, steps, 5, 'crater', 'Crater it (forward)', async () => {
    await page.locator(`[data-testid="node-list"] [data-node-id="${repo.pick}"]`).click();
    await details.locator('[data-action="crater-forward"]').click();
    await page.getByTestId('crater-summary').waitFor();
    return (await page.getByTestId('crater-summary').innerText()).replace(/\s+/g, ' ').trim();
  });

  await step(page, steps, 6, 'source', 'Open source', async () => {
    await page.getByTestId('crater-close').click();
    await page.getByTestId('crater-view').waitFor({ state: 'detached' });
    const source = page.getByTestId('source');
    await source.waitFor();
    await source.evaluate((el) => el.scrollIntoView({ block: 'center' }));
    location = (await page.getByTestId('location').innerText()).replace('open in editor', '').trim();
    return location;
  });

  await step(page, steps, 7, 'editor', 'Jump to the editor', async () => {
    await details.locator('[data-action="open-editor"]').click();
    await page.getByTestId('editor-note').waitFor();
    for (let i = 0; i < 50 && !existsSync(editorLog); i += 1) await page.waitForTimeout(100);
    if (!existsSync(editorLog)) throw new Error('the editor was not started');
    // The editor gets the absolute path; record it from the clone root so the
    // file names no home directory.
    const [file, ...rest] = JSON.parse(readFileSync(editorLog, 'utf8'));
    const relative = location.split(':')[0];
    const shown = file.replaceAll('\\', '/').endsWith(`/${relative}`) ? `<clone>/${relative}` : file;
    return `editor started with ${JSON.stringify([shown, ...rest])}`;
  });
} finally {
  await browser.close();
  child.kill();
  rmSync(scratch, { recursive: true, force: true });
}

const record = { repository: repoName, url: repo.url, commit: repo.rev, steps };
writeFileSync(join(out, `${repoName}-walkthrough.json`), `${JSON.stringify(record, null, 2)}\n`);
console.log(`saved docs/milestones/m5/${repoName}-walkthrough.json and ${steps.length} screenshots`);
