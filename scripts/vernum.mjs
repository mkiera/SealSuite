import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseVersion } from './versioning.mjs';

const packageJsonPath = join(dirname(fileURLToPath(import.meta.url)), '..', 'package.json');
const version = parseVersion(process.env.SS_BUILD_VERSION ?? JSON.parse(readFileSync(packageJsonPath, 'utf8')).version);
process.stdout.write(process.argv.includes('--raw') ? version.version : `${version.core}.0`);
