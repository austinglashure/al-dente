import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

// The built UI is embedded in the aldente binary (crates/aldente-server).
export default defineConfig({
  plugins: [react()],
  build: { outDir: 'dist', emptyOutDir: true, sourcemap: false },
  worker: { format: 'es' },
  test: { environment: 'jsdom', include: ['src/**/*.test.ts', 'src/**/*.test.tsx'] },
});
