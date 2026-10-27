import { readdir, readFile, writeFile } from 'node:fs/promises'
import { resolve, basename } from 'node:path'

const root = resolve(import.meta.dirname, '../..')
const output = resolve(import.meta.dirname, '../public')
const slug = (text) => text.toLocaleLowerCase().normalize('NFKD').replace(/[\u0300-\u036f]/g, '').replace(/[^\p{L}\p{N}]+/gu, '-').replace(/^-|-$/g, '') || 'section'
for (const locale of ['fa', 'en']) {
  const files = (await readdir(resolve(root, 'docs', locale))).filter((file) => file.endsWith('.md'))
  const records = await Promise.all(files.map(async (file) => {
    const markdown = await readFile(resolve(root, 'docs', locale, file), 'utf8')
    const headings = [...markdown.matchAll(/^#{2,3}\s+(.+)$/gm)].map((match) => ({ id: slug(match[1].replace(/[`*_]/g, '').trim()), text: match[1].replace(/[`*_]/g, '').trim() }))
    const plain = markdown.replace(/```[\w+-]*\n?([\s\S]*?)```/g, '$1').replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/\[([^\]]+)\]\([^)]*\)/g, '$1').replace(/[`*_>#|~-]/g, ' ').replace(/\s+/g, ' ').trim()
    const title = headings.find((item) => item.text)?.text || basename(file, '.md')
    const description = plain.replace(title, '').trim().slice(0, 240)
    return { id: basename(file, '.md'), title, description, headings, text: plain }
  }))
  await writeFile(resolve(output, `search-index.${locale}.json`), JSON.stringify(records))
}
