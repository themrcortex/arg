import { readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'
const root = resolve(import.meta.dirname, '../..')
const cargo = await readFile(resolve(root, 'Cargo.toml'), 'utf8')
const version = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
if (!version) throw new Error('Could not read ARG release version from Cargo.toml')
await writeFile(resolve(import.meta.dirname, '../src/version.ts'), `export const ARG_VERSION = '${version}'\n`)
