// Starts the real `aldente serve` on a copy of a fixture (outside the
// repository, so the fixture is its own root) and returns its URL.

import { spawn, type ChildProcess } from 'node:child_process';
import { cpSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export interface Server {
  url: string;
  close: () => void;
}

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const binary = process.env.ALDENTE_BIN ?? join(repoRoot, 'target', 'debug', process.platform === 'win32' ? 'aldente.exe' : 'aldente');

/** Copies `fixtures/<name>`, runs `aldente serve --no-open --json` on it,
 *  and resolves once it prints its URL.
 *  Example: `const s = await startServer('clj-mini'); await page.goto(s.url); s.close();` */
export function startServer(fixture: string): Promise<Server> {
  const dir = mkdtempSync(join(tmpdir(), 'aldente-e2e-'));
  cpSync(join(repoRoot, 'fixtures', fixture), join(dir, 'repo'), { recursive: true });
  const child: ChildProcess = spawn(binary, ['serve', '--no-open', '--json', join(dir, 'repo')], {
    env: { ...process.env, ALDENTE_CACHE_DIR: join(dir, 'cache') },
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  const close = () => {
    child.kill();
    rmSync(dir, { recursive: true, force: true });
  };
  return new Promise((resolvePromise, reject) => {
    let out = '';
    child.stdout?.on('data', (chunk: Buffer) => {
      out += chunk.toString();
      if (out.includes('\n}\n')) {
        const env = JSON.parse(out.slice(0, out.indexOf('\n}\n') + 2)) as { data: { url: string } };
        resolvePromise({ url: env.data.url, close });
      }
    });
    child.on('exit', (code) => reject(new Error(`aldente serve exited with ${code}`)));
  });
}
