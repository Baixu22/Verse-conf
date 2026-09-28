import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(readFileSync(resolve(root, 'benchmark/closeout-snapshot.json'), 'utf8'));
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

if (manifest.kind !== 'closeout-time-snapshot' || manifest.historical_preregistration !== false) {
  throw new Error('Expected a closeout-time snapshot, not a historical preregistration.');
}
if (!Array.isArray(manifest.files) || manifest.files.length === 0) {
  throw new Error('The closeout snapshot must contain files.');
}

const failures = [];
const seen = new Set();
for (const file of manifest.files) {
  if (typeof file.path !== 'string' || seen.has(file.path)) {
    failures.push(`Invalid or duplicate path: ${file.path}`);
    continue;
  }
  seen.add(file.path);
  const target = resolve(root, file.path);
  const rel = relative(root, target);
  if (isAbsolute(rel) || rel === '..' || rel.startsWith(`..${sep}`) || rel === '') {
    failures.push(`Path outside repository: ${file.path}`);
    continue;
  }
  try {
    const bytes = readFileSync(target);
    if (bytes.length !== file.bytes || sha256(bytes) !== file.sha256) {
      failures.push(`Changed: ${file.path}`);
    }
  } catch (error) {
    failures.push(`Unreadable: ${file.path} (${error.code ?? error.message})`);
  }
}

if (failures.length > 0) {
  for (const failure of failures) console.error(failure);
  console.error('Preserved evidence differs. Do not overwrite the snapshot to hide a change.');
  process.exit(1);
}
console.log(`Closeout snapshot matches: ${manifest.files.length} files (${manifest.date}).`);
console.log('This proves closeout-time integrity, not which binaries ran historical experiments.');

const checks = [
  ['benchmark/confirmation/preregistration.mjs', '--verify'],
  ...['gate-increment', 'gate-increment-round2', 'gate-increment-round3',
    'gate-increment-round4', 'gate-increment-round5'].map((round) => [
    'benchmark/gate-increment/verify.mjs', '--verify', '--dir', `benchmark/${round}`,
  ]),
];
for (const args of checks) {
  console.log(`\nnode ${args.join(' ')}`);
  const result = spawnSync(process.execPath, args, { cwd: root, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
console.log('\nPreserved snapshot and original freeze checks passed; no model calls or result writes.');
