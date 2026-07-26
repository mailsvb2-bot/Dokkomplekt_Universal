import { rmSync } from 'node:fs';

for (const path of [
  'node_modules',
  'dist',
  'target',
  'src-tauri/target',
  'test-results',
  'playwright-report',
  '.cargo-gate',
  '__pycache__',
  'scripts/__pycache__',
  'tests/__pycache__',
]) {
  rmSync(path, { recursive: true, force: true });
}
console.log('Cleaned generated directories and test artifacts.');
