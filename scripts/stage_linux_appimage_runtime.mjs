import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  chmodSync,
  closeSync,
  copyFileSync,
  mkdirSync,
  openSync,
  readFileSync,
  readSync,
  realpathSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REQUIRED_LIBRARIES = [
  'libGLESv2.so.2',
  'libEGL.so.1',
  'libGLdispatch.so.0',
];

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_DESTINATION = join(ROOT, 'src-tauri', 'target', 'appimage-runtime');

function targetArchitecture() {
  const raw = String(
    process.env.TAURI_ENV_ARCH
      || process.env.TAURI_TARGET_TRIPLE
      || process.env.CARGO_BUILD_TARGET
      || process.arch,
  ).toLowerCase();
  if (raw === 'x64' || raw.includes('x86_64')) {
    return { name: 'x86_64', elfMachine: 62 };
  }
  if (raw === 'arm64' || raw.includes('aarch64')) {
    return { name: 'aarch64', elfMachine: 183 };
  }
  throw new Error(`unsupported Linux AppImage architecture: ${raw}`);
}

function readElfMachine(path) {
  const fd = openSync(path, 'r');
  try {
    const header = Buffer.alloc(20);
    const bytesRead = readSync(fd, header, 0, header.length, 0);
    if (bytesRead < header.length || !header.subarray(0, 4).equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46]))) {
      throw new Error(`${path} is not an ELF binary`);
    }
    if (header[4] !== 2) {
      throw new Error(`${path} is not a 64-bit ELF binary`);
    }
    if (header[5] === 1) {
      return header.readUInt16LE(18);
    }
    if (header[5] === 2) {
      return header.readUInt16BE(18);
    }
    throw new Error(`${path} has an unsupported ELF byte order`);
  } finally {
    closeSync(fd);
  }
}

function ldconfigOutput() {
  const configured = process.env.DOKKOMPLEKT_LDCONFIG;
  const candidates = configured
    ? [configured]
    : ['ldconfig', '/sbin/ldconfig', '/usr/sbin/ldconfig'];
  const failures = [];
  for (const command of candidates) {
    try {
      return execFileSync(command, ['-p'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
    } catch (error) {
      failures.push(`${command}: ${error.message}`);
    }
  }
  throw new Error(`unable to execute ldconfig -p (${failures.join('; ')})`);
}

function discoverLibraries(expectedMachine) {
  const matches = new Map(REQUIRED_LIBRARIES.map((name) => [name, new Set()]));
  for (const line of ldconfigOutput().split(/\r?\n/)) {
    const [left, right] = line.split('=>', 2);
    if (!right) continue;
    const name = left.trim().split(/\s+/, 1)[0];
    if (!matches.has(name)) continue;
    const candidate = right.trim();
    try {
      const canonical = realpathSync(candidate);
      if (statSync(canonical).isFile() && readElfMachine(canonical) === expectedMachine) {
        matches.get(name).add(canonical);
      }
    } catch {
      // Ignore stale ldconfig entries and wrong-architecture multiarch candidates.
    }
  }
  const resolved = new Map();
  for (const name of REQUIRED_LIBRARIES) {
    const candidates = [...matches.get(name)].sort();
    if (candidates.length === 0) {
      throw new Error(
        `required AppImage graphics library is missing or has the wrong architecture: ${name}. `
        + 'Install libegl1, libgles2 and libglvnd0 for the packaging target.',
      );
    }
    resolved.set(name, candidates[0]);
  }
  return resolved;
}

function sha256(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function main() {
  if (process.platform !== 'linux') {
    console.log('Linux AppImage runtime staging: skipped on non-Linux host');
    return;
  }
  const architecture = targetArchitecture();
  const destination = resolve(
    process.env.DOKKOMPLEKT_APPIMAGE_RUNTIME_DIR || DEFAULT_DESTINATION,
  );
  const libraries = discoverLibraries(architecture.elfMachine);

  rmSync(destination, { recursive: true, force: true });
  mkdirSync(destination, { recursive: true });

  const manifestLibraries = [];
  for (const name of REQUIRED_LIBRARIES) {
    const source = libraries.get(name);
    const target = join(destination, name);
    copyFileSync(source, target);
    chmodSync(target, 0o755);
    const machine = readElfMachine(target);
    if (machine !== architecture.elfMachine) {
      throw new Error(`staged library architecture changed unexpectedly: ${name}`);
    }
    const sourceSize = statSync(target).size;
    manifestLibraries.push({
      name,
      elfMachine: machine,
      sourceSize,
      sourceSha256: sha256(target),
    });
    console.log(`Staged ${name} (${sourceSize} bytes) from ${source}`);
  }

  writeFileSync(
    join(destination, 'manifest.json'),
    `${JSON.stringify({
      schema: 2,
      phase: 'pre-linuxdeploy',
      generatedBy: 'scripts/stage_linux_appimage_runtime.mjs',
      targetArch: architecture.name,
      libraries: manifestLibraries,
    }, null, 2)}\n`,
    'utf8',
  );
  console.log(`Linux AppImage runtime staging complete: ${destination}`);
}

main();
