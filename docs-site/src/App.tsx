import { useEffect, useMemo, useRef, useState } from 'react'
import DOMPurify from 'dompurify'
import { marked } from 'marked'
import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp, BookOpen, ChevronLeft, Command, ExternalLink, Github, Globe2, Menu, Moon, Search, Server, ShieldCheck, Sun, X } from 'lucide-react'
import { groups, pages, type DocPage, type Locale } from './catalog'
import { Badge } from './components/ui/badge'
import { Button } from './components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from './components/ui/card'

const faContent = import.meta.glob('../../docs/fa/*.md', { query: '?raw', import: 'default', eager: true }) as Record<string, string>
const enContent = import.meta.glob('../../docs/en/*.md', { query: '?raw', import: 'default', eager: true }) as Record<string, string>
const sourceFor = (locale: Locale, file: string) => (locale === 'fa' ? faContent : enContent)[`../../docs/${locale}/${file}.md`] || ''
const getInitialPage = () => new URLSearchParams(window.location.search).get('page') || 'overview'

function parseArticle(source: string, locale: Locale) {
  const rendered = String(marked.parse(source, { gfm: true, breaks: false }))
  const doc = new DOMParser().parseFromString(rendered, 'text/html')
  const seen = new Map<string, number>()
  const toc = Array.from(doc.body.querySelectorAll('h2,h3')).map((heading) => {
    const text = heading.textContent?.trim() || ''
    const slug = text.toLocaleLowerCase().normalize('NFKD').replace(/[\u0300-\u036f]/g, '').replace(/[^\p{L}\p{N}]+/gu, '-').replace(/^-|-$/g, '') || 'section'
    const count = seen.get(slug) || 0
    seen.set(slug, count + 1)
    const id = count ? `${slug}-${count + 1}` : slug
    heading.id = id
    return { id, text, depth: Number(heading.tagName.slice(1)) }
  })
  doc.body.querySelectorAll('pre').forEach((pre) => {
    const code = pre.querySelector('code')
    const language = code?.className.match(/language-([\w+-]+)/)?.[1] || 'text'
    const wrapper = doc.createElement('div')
    wrapper.className = 'doc-code'
    const top = doc.createElement('div')
    top.className = 'doc-code-top'
    const label = doc.createElement('span')
    label.textContent = language
    const button = doc.createElement('button')
    button.type = 'button'
    button.dataset.copyCode = 'true'
    button.textContent = locale === 'fa' ? 'کپی' : 'Copy'
    top.append(label, button)
    pre.replaceWith(wrapper)
    wrapper.append(top, pre)
  })
  doc.body.querySelectorAll('a[href^="http"]').forEach((link) => {
    link.setAttribute('target', '_blank')
    link.setAttribute('rel', 'noopener noreferrer')
  })
  return { html: DOMPurify.sanitize(doc.body.innerHTML), toc }
}

const copyText = async (value: string) => {
  try { await navigator.clipboard.writeText(value); return true } catch { return false }
}

export default function App() {
  const [locale, setLocale] = useState<Locale>(() => localStorage.getItem('arg-docs-locale') === 'en' ? 'en' : 'fa')
  const [theme, setTheme] = useState<'dark' | 'light'>(() => localStorage.getItem('arg-docs-theme') === 'light' ? 'light' : 'dark')
  const [pageId, setPageId] = useState(getInitialPage)
  const [query, setQuery] = useState('')
  const [mobileNav, setMobileNav] = useState(false)
  const searchRef = useRef<HTMLInputElement>(null)
  const page = pages.find((item) => item.id === pageId) || pages[0]
  const article = useMemo(() => parseArticle(sourceFor(locale, page.file), locale), [locale, page.file])
  const isFa = locale === 'fa'
  const visiblePages = useMemo(() => pages.filter((item) => `${item.title[locale]} ${item.summary[locale]} ${sourceFor(locale, item.file)}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())), [locale, query])
  const texts = isFa ? {
    title: 'مستندات ARG', subtitle: 'راهنمای کامل مدیریت VPS', search: 'جست‌وجو در مستندات…', searchHint: 'برای جست‌وجو / را بزن', toc: 'در این صفحه', start: 'شروع سریع', github: 'مخزن GitHub', support: 'پشتیبانی', language: 'English', updated: 'همگام با نسخهٔ پایدار', empty: 'صفحه‌ای پیدا نشد.', menu: 'فهرست مستندات', copy: 'کپی شد', copied: 'کپی شد', previous: 'صفحهٔ قبل', next: 'صفحهٔ بعد', allDocs: 'صفحه', close: 'بستن فهرست', count: 'راهنما', disclaimer: 'مستندات بر اساس قابلیت‌های موجود مخزن نوشته شده‌اند. مواردی که هنوز پیاده‌سازی نشده‌اند صریحاً مشخص شده‌اند.', top: 'بازگشت به بالا',
  } : {
    title: 'ARG Documentation', subtitle: 'Complete VPS management guide', search: 'Search documentation…', searchHint: 'Press / to search', toc: 'On this page', start: 'Quick start', github: 'GitHub repository', support: 'Support', language: 'فارسی', updated: 'Tracks the stable release', empty: 'No pages found.', menu: 'Documentation', close: 'Close navigation', copied: 'Copied', previous: 'Previous page', next: 'Next page', allDocs: 'pages', count: 'Guides', disclaimer: 'This documentation reflects features present in the repository. Planned or unavailable features are clearly identified.', top: 'Back to top',
  }

  useEffect(() => {
    document.documentElement.lang = locale
    document.documentElement.dir = isFa ? 'rtl' : 'ltr'
    document.documentElement.dataset.theme = theme
    document.title = `${page.title[locale]} — ${texts.title}`
    localStorage.setItem('arg-docs-locale', locale)
    localStorage.setItem('arg-docs-theme', theme)
  }, [locale, isFa, page, texts.title, theme])

  useEffect(() => {
    const onPop = () => setPageId(getInitialPage())
    const onKey = (event: KeyboardEvent) => {
      if (event.key === '/' && !['INPUT', 'TEXTAREA'].includes((event.target as HTMLElement).tagName)) { event.preventDefault(); searchRef.current?.focus() }
      if (event.key === 'Escape') setMobileNav(false)
    }
    window.addEventListener('popstate', onPop)
    window.addEventListener('keydown', onKey)
    return () => { window.removeEventListener('popstate', onPop); window.removeEventListener('keydown', onKey) }
  }, [])

  function navigate(item: DocPage) {
    setPageId(item.id)
    setQuery('')
    setMobileNav(false)
    history.pushState(null, '', `?page=${item.id}`)
    window.scrollTo({ top: 0, behavior: 'smooth' })
  }

  function setLanguage(next: Locale) {
    setLocale(next)
    setQuery('')
  }

  const index = pages.findIndex((item) => item.id === page.id)
  const previous = pages[index - 1]
  const next = pages[index + 1]
  const groupsList = groups[locale]

  const sidebar = <>
    <div className="sidebar-heading"><div className="sidebar-heading-icon"><BookOpen size={17}/></div><div><p>{texts.menu}</p><span>{pages.length} {texts.allDocs}</span></div><button className="mobile-close" onClick={() => setMobileNav(false)} aria-label={texts.close}><X size={18}/></button></div>
    <div className="side-search"><Search size={15}/><input aria-label={texts.search} placeholder={texts.search} value={query} onChange={(event) => setQuery(event.target.value)}/><kbd>/</kbd></div>
    <nav className="side-nav" aria-label={texts.menu}>
      {groupsList.map((groupName, groupIndex) => {
        const groupPages = visiblePages.filter((item) => Number(item.group) === groupIndex)
        if (!groupPages.length) return null
        return <section key={groupName} className="nav-group"><h2>{groupName}</h2>{groupPages.map((item) => <button key={item.id} className={`nav-link ${item.id === page.id ? 'active' : ''}`} onClick={() => navigate(item)} aria-current={item.id === page.id ? 'page' : undefined}><span className="nav-marker"/><span className="nav-link-text">{item.title[locale]}</span>{item.id === page.id && <ChevronLeft size={14}/>}</button>)}</section>
      })}
      {!visiblePages.length && <p className="empty-search">{texts.empty}</p>}
    </nav>
    <div className="sidebar-bottom"><div className="sidebar-bottom-icon"><ShieldCheck size={15}/></div><p>{texts.disclaimer}</p></div>
  </>

  return <div className="docs-app" dir={isFa ? 'rtl' : 'ltr'}>
    <a className="skip-link" href="#article">{isFa ? 'رفتن به محتوای اصلی' : 'Skip to content'}</a>
    <header className="topbar" id="top">
      <div className="topbar-brand"><button className="mobile-menu" onClick={() => setMobileNav(true)} aria-label={texts.menu}><Menu size={19}/></button><a href="?page=overview" onClick={(event) => { event.preventDefault(); navigate(pages[0]) }} className="brand-lockup"><span className="brand-mark">A</span><span><strong className="brand-name">ARG</strong><small>{texts.title}</small></span></a><span className="top-divider"/><Badge>v0.6.5</Badge></div>
      <div className="topbar-actions"><label className="top-search"><Search size={16}/><input ref={searchRef} aria-label={texts.search} placeholder={texts.search} value={query} onChange={(event) => { setQuery(event.target.value); setMobileNav(true) }}/><kbd>/</kbd></label><Button variant="outline" size="sm" onClick={() => setLanguage(isFa ? 'en' : 'fa')} aria-label="Change language"><Globe2 size={15}/>{texts.language}</Button><Button variant="ghost" size="icon" onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')} aria-label={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}>{theme === 'dark' ? <Sun size={17}/> : <Moon size={17}/>}</Button><a className="icon-link" href="https://github.com/themrcortex/arg" target="_blank" rel="noreferrer" aria-label={texts.github}><Github size={17}/></a></div>
    </header>
    <div className="docs-layout">
      <aside className="desktop-sidebar">{sidebar}</aside>
      <main id="article" className="article-column" key={`${locale}-${page.id}`}>
        <div className="breadcrumbs"><span>{groupsList[Number(page.group)]}</span><ChevronLeft size={13}/><span>{page.title[locale]}</span></div>
        {page.id === 'overview' && <section className="hero-card"><div className="hero-grid"/><div className="hero-copy"><Badge><span className="status-dot"/>{isFa ? 'راهنمای رسمی' : 'OFFICIAL DOCUMENTATION'}</Badge><h1>{isFa ? 'سرور را روشن ببین.' : 'See your server clearly.'}<br/><span>{isFa ? 'با ARG.' : 'With ARG.'}</span></h1><p>{isFa ? 'راهنمای عملی و دقیق برای نصب، ایمن‌سازی و مدیریت سرور لینوکسی با ARG.' : 'A practical, detailed guide to installing, securing, and operating a Linux server with ARG.'}</p><div className="hero-actions"><Button onClick={() => navigate(pages[1])}>{texts.start}<ArrowDown size={15}/></Button><a href="https://github.com/themrcortex/arg" target="_blank" rel="noreferrer" className="hero-github"><Github size={15}/>{texts.github}<ExternalLink size={12}/></a></div></div><div className="hero-art" aria-hidden="true"><div className="orbit orbit-one"/><div className="orbit orbit-two"/><div className="server-glyph"><Server size={54} strokeWidth={1.2}/><i/><i/></div><div className="hero-art-label">YOUR SERVER · CLEARLY</div></div><div className="hero-meta"><span><code>Rust</code><small>{isFa ? 'Backend' : 'Backend'}</small></span><span><code>React</code><small>Web panel</small></span><span><code>systemd</code><small>{isFa ? 'سرویس' : 'Service'}</small></span></div></section>}
        <div className="article-heading"><div><p className="eyebrow"><span className="eyebrow-dot"/>{groupsList[Number(page.group)]}</p><h1>{page.title[locale]}</h1><p className="article-summary">{page.summary[locale]}</p></div><span className="article-index">{String(index + 1).padStart(2, '0')} <i>/</i> {String(pages.length).padStart(2, '0')}</span></div>
        {article.toc.length > 0 && <details className="mobile-toc"><summary>{texts.toc}<ChevronLeft size={14}/></summary><nav>{article.toc.map((item) => <a key={item.id} className={item.depth === 3 ? 'toc-sub' : ''} href={`#${item.id}`} onClick={() => setMobileNav(false)}>{item.text}</a>)}</nav></details>}
        <div className="article-content" dangerouslySetInnerHTML={{ __html: article.html }} onClick={async (event) => {
          const target = event.target as HTMLElement
          const copyButton = target.closest<HTMLButtonElement>('[data-copy-code]')
          if (copyButton) {
            const code = copyButton.closest('.doc-code')?.querySelector('code')?.textContent || ''
            const ok = await copyText(code)
            if (ok) { copyButton.textContent = texts.copied; setTimeout(() => { copyButton.textContent = isFa ? 'کپی' : 'Copy' }, 1300) }
          }
        }}/>
        <nav className="page-pager" aria-label={isFa ? 'ناوبری صفحات' : 'Page navigation'}>
          {previous ? <button onClick={() => navigate(previous)}><span><ArrowRight size={15}/>{texts.previous}</span><strong>{previous.title[locale]}</strong></button> : <span/>}
          {next ? <button className="pager-next" onClick={() => navigate(next)}><span>{texts.next}<ArrowLeft size={15}/></span><strong>{next.title[locale]}</strong></button> : <span/>}
        </nav>
        <footer className="article-footer"><span>ARG · VPS CONTROL CENTER</span><span>{isFa ? 'مستندات متن‌باز · نسخهٔ 0.6.5' : 'Open-source docs · v0.6.5'}</span></footer>
      </main>
      <aside className="toc-column"><div className="toc-sticky"><h2>{texts.toc}</h2>{article.toc.length ? <nav>{article.toc.map((item) => <a key={item.id} className={item.depth === 3 ? 'toc-sub' : ''} href={`#${item.id}`}>{item.text}</a>)}</nav> : <p>{isFa ? 'این صفحه سرفصل فرعی ندارد.' : 'No subsections on this page.'}</p>}<Card className="help-card"><CardHeader><div className="help-icon"><Command size={16}/></div><CardTitle>{isFa ? 'سریع به نتیجه برس' : 'Find your way'}</CardTitle></CardHeader><CardContent><p>{isFa ? 'از / برای جست‌وجو استفاده کن یا کلیدهای جهت برای پیمایش صفحه را به کار ببر.' : 'Use / to search the guides, or use the previous and next controls at the bottom.'}</p><a href="https://github.com/themrcortex/arg/issues" target="_blank" rel="noreferrer">{texts.support}<ExternalLink size={12}/></a></CardContent></Card></div></aside>
    </div>
    {mobileNav && <div className="mobile-overlay" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) setMobileNav(false) }}><aside className="mobile-sidebar">{sidebar}</aside></div>}
    <a href="#top" className="back-top" onClick={(event) => { event.preventDefault(); window.scrollTo({ top: 0, behavior: 'smooth' }) }} aria-label={texts.top}><ArrowUp size={16}/></a>
  </div>
}
