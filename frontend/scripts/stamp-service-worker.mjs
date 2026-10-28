import { createHash } from 'node:crypto'
import { readdir, readFile, writeFile } from 'node:fs/promises'
import { join, relative, sep } from 'node:path'

const dist = new URL('../dist/', import.meta.url)
const workerPath = new URL('../dist/service-worker.js', import.meta.url)
const marker = '__ARG_BUILD_ID__'

async function collectFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true })
  const files = []
  for (const entry of entries) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) files.push(...await collectFiles(path))
    else if (entry.isFile() && relative(dist.pathname, path).split(sep).join('/') !== 'service-worker.js') files.push(path)
  }
  return files
}

const hash = createHash('sha256')
const files = (await collectFiles(dist.pathname)).sort()
if (!files.length) throw new Error('Frontend build output is empty; cannot stamp the service worker.')

for (const file of files) {
  hash.update(relative(dist.pathname, file).split(sep).join('/'))
  hash.update('\0')
  hash.update(await readFile(file))
  hash.update('\0')
}

const worker = await readFile(workerPath, 'utf8')
if (!worker.includes(marker)) throw new Error(`Service worker build marker ${marker} is missing.`)
await writeFile(workerPath, worker.replaceAll(marker, hash.digest('hex').slice(0, 16)))
