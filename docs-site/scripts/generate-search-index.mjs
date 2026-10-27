import { chmod, mkdir, readFile, rm, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '../..')
const output = resolve(import.meta.dirname, '../public')
const catalog = await readFile(resolve(import.meta.dirname, '../src/catalog.ts'), 'utf8')
const slug = (text) => text.toLocaleLowerCase().normalize('NFKD').replace(/[\u0300-\u036f]/g, '').replace(/[^\p{L}\p{N}]+/gu, '-').replace(/^-|-$/g, '') || 'section'
const entries = [...catalog.matchAll(/\{ id: '([^']+)', group: '([^']+)', title: \{ fa: '([^']*)', en: '([^']*)' \}, summary: \{ fa: '([^']*)', en: '([^']*)' \}, file: '([^']+)' \}/g)]
if (!entries.length) throw new Error('Could not read documentation catalog')

const contentRoot = resolve(output, 'content')
await rm(contentRoot, { recursive: true, force: true })
await mkdir(contentRoot, { recursive: true, mode: 0o755 })
await chmod(contentRoot, 0o755)

for (const locale of ['fa', 'en']) {
  const contentDirectory = resolve(contentRoot, locale)
  await mkdir(contentDirectory, { mode: 0o755 })
  await chmod(contentDirectory, 0o755)
  const records = await Promise.all(entries.map(async ([, id, , titleFa, titleEn, summaryFa, summaryEn, file]) => {
    const markdown = await readFile(resolve(root, 'docs', locale, `${file}.md`), 'utf8')
    const contentPath = resolve(contentDirectory, `${file}.md`)
    await writeFile(contentPath, markdown, { mode: 0o644 })
    await chmod(contentPath, 0o644)
    const seen = new Map()
    const headings = [...markdown.matchAll(/^#{2,6}\s+(.+)$/gm)].map((match) => {
      const text = match[1].replace(/[`*_]/g, '').trim()
      const base = slug(text), occurrence = seen.get(base) || 0
      seen.set(base, occurrence + 1)
      return { id: occurrence ? `${base}-${occurrence + 1}` : base, text }
    })
    const plain = markdown.replace(/```[\w+-]*\n?([\s\S]*?)```/g, '$1').replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/\[([^\]]+)\]\([^)]*\)/g, '$1').replace(/[`*_>#|~-]/g, ' ').replace(/\s+/g, ' ').trim()
    return { id, title: locale === 'fa' ? titleFa : titleEn, description: locale === 'fa' ? summaryFa : summaryEn, headings, text: plain }
  }))
  const indexPath = resolve(output, `search-index.${locale}.json`)
  await writeFile(indexPath, JSON.stringify(records), { mode: 0o644 })
  await chmod(indexPath, 0o644)
}
