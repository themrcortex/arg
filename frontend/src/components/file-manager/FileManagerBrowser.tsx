import { useMemo, useState, type ReactNode } from 'react'
import { ArrowUp, FileText, Folder, Link2, Search } from 'lucide-react'
import { Badge } from '../ui/badge'
import { Button } from '../ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card'
import { Input } from '../ui/input'
import { Skeleton } from '../ui/skeleton'

export type FileManagerEntry = {
  id: string
  name: string
  path: string
  kind: 'directory' | 'file' | 'symlink' | 'other'
  sizeLabel?: string
  modifiedLabel?: string
  detail?: string
}

type Props = {
  title: string
  description?: string
  path?: string
  parentPath?: string | null
  entries: FileManagerEntry[]
  loading?: boolean
  emptyMessage?: string
  toolbar?: ReactNode
  children?: ReactNode
  pagination?: ReactNode
  onNavigate?: (path: string) => void
  onOpen?: (entry: FileManagerEntry) => void
  renderActions?: (entry: FileManagerEntry) => ReactNode
}

const kindLabels = { directory: 'پوشه', file: 'فایل', symlink: 'پیوند', other: 'سایر' }

export function FileManagerBrowser({ title, description, path, parentPath, entries, loading = false, emptyMessage = 'این پوشه خالی است.', toolbar, children, pagination, onNavigate, onOpen, renderActions }: Props) {
  const [query, setQuery] = useState('')
  const visibleEntries = useMemo(() => entries.filter((entry) => entry.name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())), [entries, query])
  const activate = (entry: FileManagerEntry) => {
    if (entry.kind === 'directory' && onNavigate) onNavigate(entry.path)
    else onOpen?.(entry)
  }
  const nameCell = (entry: FileManagerEntry) => {
    const content = <><span className="mt-0.5 shrink-0 text-primary/90">{entry.kind === 'directory' ? <Folder size={16}/> : entry.kind === 'symlink' ? <Link2 size={16}/> : <FileText size={16}/>}</span><span className="min-w-0"><span dir="ltr" className="block max-w-full break-all text-left font-mono text-xs">{entry.name}</span>{entry.detail && <span className="mt-1 block max-w-full break-all text-[10px] text-muted-foreground" dir="ltr">{entry.detail}</span>}</span>{entry.kind === 'symlink' && <Badge variant="warning">symlink</Badge>}</>
    const canActivate = entry.kind === 'directory' ? !!onNavigate : !!onOpen
    return canActivate ? <button type="button" className="flex min-w-0 items-start gap-2 text-right hover:text-primary" onClick={() => activate(entry)}>{content}</button> : <span className="flex min-w-0 items-start gap-2 text-right">{content}</span>
  }
  return <Card className="min-w-0 overflow-hidden rounded-[14px] border-border"><CardHeader className="flex-row flex-wrap items-start justify-between gap-3 space-y-0 p-4 sm:p-5"><div className="min-w-0"><CardTitle className="text-sm">{title}</CardTitle>{description && <CardDescription className="mt-1 leading-5">{description}</CardDescription>}{path && <code dir="ltr" className="mt-2 block truncate text-left font-mono text-[10px] text-muted-foreground">{path}</code>}</div>{toolbar && <div className="flex flex-wrap gap-2">{toolbar}</div>}</CardHeader><CardContent className="space-y-3 p-3 pt-0 sm:p-4 sm:pt-0">
    {children}
    <div className="flex flex-wrap items-center justify-between gap-2"><div className="flex min-w-0 flex-1 items-center gap-2">{parentPath && onNavigate && <Button type="button" size="sm" variant="outline" onClick={() => onNavigate(parentPath)}><ArrowUp size={14}/>یک سطح بالاتر</Button>}<div className="relative min-w-[140px] flex-1"><Search size={14} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input value={query} onChange={(event) => setQuery(event.target.value)} aria-label="جست‌وجوی فایل و پوشه" placeholder="جست‌وجو در این مسیر…" className="h-9 pr-9"/></div></div><span className="shrink-0 text-[10px] text-muted-foreground">{visibleEntries.length.toLocaleString('en-US')} مورد</span></div>
    {loading ? <div className="space-y-2">{Array.from({ length: 6 }, (_, index) => <Skeleton key={index} className="h-12 rounded-xl"/>)}</div> : <>
      <div className="hidden overflow-x-auto md:block"><table className="w-full min-w-[560px] text-sm"><thead><tr className="border-b border-border text-right text-[10px] text-muted-foreground"><th className="px-3 py-2.5 font-medium">نام</th><th className="px-3 py-2.5 font-medium">نوع</th><th className="px-3 py-2.5 font-medium">اندازه</th><th className="px-3 py-2.5 font-medium">تغییر</th><th className="px-3 py-2.5 text-left font-medium">عملیات</th></tr></thead><tbody>{visibleEntries.map((entry) => <tr key={entry.id} className="group border-b border-border/60 transition-colors hover:bg-white/[.025]"><td className="max-w-[380px] px-3 py-2.5">{nameCell(entry)}</td><td className="px-3 py-2.5 text-xs text-muted-foreground">{kindLabels[entry.kind]}</td><td dir="ltr" className="px-3 py-2.5 text-left font-mono text-[10px] text-muted-foreground">{entry.sizeLabel || '—'}</td><td className="px-3 py-2.5 text-xs text-muted-foreground">{entry.modifiedLabel || '—'}</td><td><div className="flex justify-end gap-0.5 px-2">{renderActions?.(entry)}</div></td></tr>)}</tbody></table></div>
      <div className="space-y-2 md:hidden">{visibleEntries.map((entry) => <article key={entry.id} className="rounded-xl border border-border/80 bg-background/35 p-3"><div className="flex min-w-0 items-start justify-between gap-2">{nameCell(entry)}<span className="shrink-0 text-[10px] text-muted-foreground">{entry.modifiedLabel}</span></div><div className="mt-2 flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-2"><span className="text-[10px] text-muted-foreground">{kindLabels[entry.kind]}{entry.sizeLabel ? ` · ${entry.sizeLabel}` : ''}</span><div className="flex flex-wrap justify-end gap-0.5">{renderActions?.(entry)}</div></div></article>)}</div>
      {!visibleEntries.length && <div className="grid min-h-36 place-items-center text-center text-sm text-muted-foreground">{query ? 'موردی با این نام پیدا نشد.' : emptyMessage}</div>}
    </>}
    {pagination}
  </CardContent></Card>
}
