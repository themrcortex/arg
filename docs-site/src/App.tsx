import { useDeferredValue, useEffect, useMemo, useRef, useState } from 'react'
import DOMPurify from 'dompurify'
import { marked } from 'marked'
import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp, BookOpen, ChevronDown, ChevronLeft, Command, Container, Database, ExternalLink, Github, Globe2, Menu, Moon, Network, Search, Server, Settings2, Shield, Sun, ChartNoAxesCombined, Code2, X } from 'lucide-react'
import { groups, navigation, pages, readingOrder, pageNeighbors, pageTrail, type DocPage, type Locale } from './catalog'
import { Badge } from './components/ui/badge'
import { ARG_VERSION } from './version'
import { Button } from './components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from './components/ui/card'

const faContent = import.meta.glob('../../docs/fa/*.md', { query: '?raw', import: 'default' }) as Record<string, () => Promise<string>>
const enContent = import.meta.glob('../../docs/en/*.md', { query: '?raw', import: 'default' }) as Record<string, () => Promise<string>>
const sourceFor = async (locale: Locale, file: string) => {
  const load = (locale === 'fa' ? faContent : enContent)[`../../docs/${locale}/${file}.md`]
  if (!load) throw new Error(`Documentation article not found: ${locale}/${file}`)
  return load()
}
const getInitialPage = () => {
  const requested = new URLSearchParams(window.location.search).get('page')
  return pages.some((item) => item.id === requested) ? requested! : 'overview'
}

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

type SearchRecord = { id: string; title: string; description: string; headings: { id: string; text: string }[]; text: string }
const normalizeSearch = (value: string) => value.toLocaleLowerCase().replace(/[يى]/g, 'ی').replace(/ك/g, 'ک').replace(/[\u064B-\u065F\u0670]/g, '').trim()
const highlightText = (text: string, query: string) => {
  const at = text.toLocaleLowerCase().indexOf(query.toLocaleLowerCase())
  if (at < 0 || !query) return text
  return <>{text.slice(0, at)}<mark>{text.slice(at, at + query.length)}</mark>{text.slice(at + query.length)}</>
}

export default function App() {
  const [locale, setLocale] = useState<Locale>(() => localStorage.getItem('arg-docs-locale') === 'en' ? 'en' : 'fa')
  const [theme, setTheme] = useState<'dark' | 'light'>(() => localStorage.getItem('arg-docs-theme') === 'light' ? 'light' : 'dark')
  const [pageId, setPageId] = useState(getInitialPage)
  const [query, setQuery] = useState('')
  const [mobileNav, setMobileNav] = useState(false)
  const [pageLoading, setPageLoading] = useState(false)
  const [articleSource, setArticleSource] = useState('')
  const [articleLoading, setArticleLoading] = useState(true)
  const [articleError, setArticleError] = useState(false)
  const [articleReload, setArticleReload] = useState(0)
  const [searchIndex, setSearchIndex] = useState<SearchRecord[] | null>(null)
  const [showBackTop, setShowBackTop] = useState(false)
  const [activeHeading, setActiveHeading] = useState('')
  const [readingProgress, setReadingProgress] = useState(0)
  const [targetAnchor, setTargetAnchor] = useState(() => decodeURIComponent(window.location.hash.slice(1)))
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(() => { try { return new Set(JSON.parse(localStorage.getItem('arg-docs-expanded') || '[]') as string[]) } catch { return new Set() } })
  const searchRef = useRef<HTMLInputElement>(null)
  const mobileSearchRef = useRef<HTMLInputElement>(null)
  const previousFocus = useRef<HTMLElement | null>(null)
  const transitionTimer = useRef<number | null>(null)
  const previousPageId = useRef(pageId)
  const deferredQuery = useDeferredValue(query)
  const page = pages.find((item) => item.id === pageId) || pages[0]
  const article = useMemo(() => parseArticle(articleSource, locale), [articleSource, locale])
  const isFa = locale === 'fa'
  const visiblePages = useMemo(() => pages.filter((item) => `${item.title[locale]} ${item.summary[locale]}`.toLocaleLowerCase().includes(deferredQuery.trim().toLocaleLowerCase())), [locale, deferredQuery])
  const texts = isFa ? {
    title: 'مستندات ARG', subtitle: 'راهنمای کامل مدیریت VPS', search: 'جست‌وجو در مستندات…', searchHint: 'برای جست‌وجو / را بزن', toc: 'در این صفحه', start: 'شروع سریع', github: 'مخزن GitHub', support: 'پشتیبانی', language: 'English', updated: 'همگام با نسخهٔ پایدار', empty: 'صفحه‌ای پیدا نشد.', menu: 'فهرست مستندات', copy: 'کپی شد', copied: 'کپی شد', previous: 'صفحهٔ قبل', next: 'صفحهٔ بعد', allDocs: 'صفحه', close: 'بستن فهرست', count: 'راهنما', disclaimer: 'مستندات بر اساس قابلیت‌های موجود مخزن نوشته شده‌اند. مواردی که هنوز پیاده‌سازی نشده‌اند صریحاً مشخص شده‌اند.', top: 'بازگشت به بالا', loadingPage: 'در حال بارگذاری راهنما…', articleError: 'بارگذاری این راهنما ناموفق بود.', retry: 'تلاش دوباره',
  } : {
    title: 'ARG Documentation', subtitle: 'Complete VPS management guide', search: 'Search documentation…', searchHint: 'Press / to search', toc: 'On this page', start: 'Quick start', github: 'GitHub repository', support: 'Support', language: 'فارسی', updated: 'Tracks the stable release', empty: 'No pages found.', menu: 'Documentation', close: 'Close navigation', copied: 'Copied', previous: 'Previous page', next: 'Next page', allDocs: 'pages', count: 'Guides', disclaimer: 'This documentation reflects features present in the repository. Planned or unavailable features are clearly identified.', top: 'Back to top', loadingPage: 'Loading guide…', articleError: 'Could not load this guide.', retry: 'Try again',
  }

  useEffect(() => {
    let cancelled = false
    setArticleSource('')
    setArticleLoading(true)
    setArticleError(false)
    void sourceFor(locale, page.file).then((source) => {
      if (cancelled) return
      setArticleSource(source)
      setArticleLoading(false)
    }).catch(() => {
      if (cancelled) return
      setArticleLoading(false)
      setArticleError(true)
    })
    return () => { cancelled = true }
  }, [locale, page.file, articleReload])

  useEffect(() => {
    if (query.trim().length < 2 || searchIndex) return
    let cancelled = false
    void fetch(`${import.meta.env.BASE_URL}search-index.${locale}.json`).then((response) => response.json()).then((records: SearchRecord[]) => { if (!cancelled) setSearchIndex(records) }).catch(() => undefined)
    return () => { cancelled = true }
  }, [query, locale, searchIndex])

  useEffect(() => {
    document.documentElement.lang = locale
    document.documentElement.dir = isFa ? 'rtl' : 'ltr'
    document.documentElement.dataset.theme = theme
    document.title = `${page.title[locale]} — ${texts.title}`
    localStorage.setItem('arg-docs-locale', locale)
    localStorage.setItem('arg-docs-theme', theme)
  }, [locale, isFa, page, texts.title, theme])

  useEffect(() => {
    const onPop = () => {
      startPageTransition()
      setPageId(getInitialPage())
      setTargetAnchor(decodeURIComponent(window.location.hash.slice(1)))
      setQuery('')
      setMobileNav(false)
    }
    const onKey = (event: KeyboardEvent) => {
      if (event.key === '/' && !['INPUT', 'TEXTAREA'].includes((event.target as HTMLElement).tagName)) {
        event.preventDefault()
        if (window.matchMedia('(max-width: 760px)').matches) {
          setMobileNav(true)
          requestAnimationFrame(() => mobileSearchRef.current?.focus())
        } else searchRef.current?.focus()
      }
      if (event.key === 'Escape') { setMobileNav(false); setQuery('') }
      if (event.key === 'ArrowDown' && (event.target as HTMLElement).matches('.top-search input, .side-search input') && query.trim().length >= 2) {
        event.preventDefault()
        document.querySelector<HTMLElement>('.search-results button, .inline-search-results button')?.focus()
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        if (window.matchMedia('(max-width: 760px)').matches) setMobileNav(true)
        requestAnimationFrame(() => (window.matchMedia('(max-width: 760px)').matches ? mobileSearchRef.current : searchRef.current)?.focus())
      }
    }
    window.addEventListener('popstate', onPop)
    window.addEventListener('keydown', onKey)
    return () => { window.removeEventListener('popstate', onPop); window.removeEventListener('keydown', onKey) }
  }, [])

  useEffect(() => {
    if (articleLoading || !article.toc.length) return
    const observer = new IntersectionObserver((entries) => {
      const visible = entries.filter((entry) => entry.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0]
      if (visible) setActiveHeading(visible.target.id)
    }, { rootMargin: '-12% 0px -76% 0px', threshold: 0 })
    article.toc.forEach((item) => { const heading = document.getElementById(item.id); if (heading) observer.observe(heading) })
    return () => observer.disconnect()
  }, [article.toc, articleLoading, page.id])

  useEffect(() => {
    const onScroll = () => {
      setShowBackTop(window.scrollY > 420)
      const max = document.documentElement.scrollHeight - window.innerHeight
      setReadingProgress(max > 0 ? Math.min(100, Math.max(0, Math.round(window.scrollY / max * 100))) : 0)
    }
    onScroll()
    window.addEventListener('scroll', onScroll, { passive: true })
    return () => window.removeEventListener('scroll', onScroll)
  }, [])

  useEffect(() => () => {
    if (transitionTimer.current !== null) window.clearTimeout(transitionTimer.current)
  }, [])

  useEffect(() => {
    if (previousPageId.current === page.id) return
    previousPageId.current = page.id
    if (targetAnchor) return
    window.scrollTo({ top: 0, behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' })
  }, [page.id, targetAnchor])

  useEffect(() => {
    if (!targetAnchor || articleLoading) return
    const frame = requestAnimationFrame(() => {
      document.getElementById(targetAnchor)?.scrollIntoView({ block: 'start' })
      setTargetAnchor('')
    })
    return () => cancelAnimationFrame(frame)
  }, [targetAnchor, articleLoading, page.id])

  useEffect(() => {
    const [category, section] = pageTrail(page.id)
    setExpandedGroups((current) => new Set(current).add(category?.id || page.group).add(`${category?.id || page.group}:${section?.id || ''}`))
    requestAnimationFrame(() => {
      const sidebar = mobileNav ? '.mobile-sidebar' : '.desktop-sidebar'
      document.querySelector(`${sidebar} .nav-link.active`)?.scrollIntoView({ block: 'nearest' })
    })
  }, [page.group, page.id, mobileNav])

  useEffect(() => {
    if (!query.trim()) return
    const matchingGroups = new Set(visiblePages.flatMap((item) => { const [category, section] = pageTrail(item.id); return [category?.id || item.group, `${category?.id || item.group}:${section?.id || ''}`] }))
    setExpandedGroups((current) => new Set([...current, ...matchingGroups]))
  }, [query, visiblePages, page.group])

  useEffect(() => { localStorage.setItem('arg-docs-expanded', JSON.stringify([...expandedGroups])) }, [expandedGroups])

  useEffect(() => {
    if (!mobileNav) return
    previousFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const oldOverflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    requestAnimationFrame(() => mobileSearchRef.current?.focus())
    return () => {
      document.body.style.overflow = oldOverflow
      previousFocus.current?.focus()
    }
  }, [mobileNav])

  function navigate(item: DocPage, anchor?: string) {
    if (item.id !== page.id) startPageTransition()
    setPageId(item.id)
    setTargetAnchor(anchor || '')
    setQuery('')
    setMobileNav(false)
    history.pushState(null, '', `?page=${item.id}${anchor ? `#${anchor}` : ''}`)
  }

  function setLanguage(next: Locale) {
    startPageTransition()
    setLocale(next)
    setSearchIndex(null)
    setTargetAnchor(decodeURIComponent(window.location.hash.slice(1)))
    setQuery('')
    window.scrollTo({ top: 0, behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' })
  }

  function startPageTransition() {
    setPageLoading(true)
    if (transitionTimer.current !== null) window.clearTimeout(transitionTimer.current)
    transitionTimer.current = window.setTimeout(() => {
      setPageLoading(false)
      transitionTimer.current = null
    }, 420)
  }

  function toggleExpanded(id: string) {
    setExpandedGroups((current) => { const next = new Set(current); next.has(id) ? next.delete(id) : next.add(id); return next })
  }

  function renderSidebar(mobile = false) {
    return <>
      <div className="sidebar-heading"><div className="sidebar-heading-icon"><BookOpen size={16}/></div><div className="min-w-0"><p>{texts.menu}</p><span>{readingOrder.length} {texts.allDocs}</span></div><button className="mobile-close" onClick={() => setMobileNav(false)} aria-label={texts.close}><X size={18}/></button></div>
      <label className="side-search"><Search size={15}/><input ref={mobile ? mobileSearchRef : undefined} aria-label={texts.search} placeholder={texts.search} value={query} onChange={(event) => setQuery(event.target.value)}/><kbd>/</kbd></label>
      <nav className="side-nav" aria-label={texts.menu}>
        {query.trim().length >= 2 && (searchIndex ? rankedSearchResults.length ? <div className="inline-search-results">{rankedSearchResults.map((result) => {
          const target = pages.find((item) => item.id === result.id)!
          return <button key={result.id} onClick={() => navigate(target, result.heading?.id)}><small>{groupsList[target.group]}</small><strong>{highlightText(target.title[locale], query.trim())}</strong><span>{highlightText(result.heading?.text || result.excerpt || target.summary[locale], query.trim())}</span></button>
        })}</div> : <p className="empty-search">{isFa ? 'نتیجه‌ای پیدا نشد.' : 'No results found.'}</p> : <p className="empty-search">{isFa ? 'در حال آماده‌سازی جست‌وجو…' : 'Preparing search…'}</p>)}
        {navigation.map((category) => {
          const visibleSections = category.children.map((section) => ({ ...section, pages: section.pages.filter((item) => visiblePages.some((visible) => visible.id === item.id)) })).filter((section) => section.pages.length)
          if (!visibleSections.length) return null
          const categoryOpen = expandedGroups.has(category.id)
          const iconMap = { gettingStarted: BookOpen, dashboard: ChartNoAxesCombined, network: Network, docker: Container, data: Database, system: Settings2, security: Shield, reference: Code2 }
          const CategoryIcon = iconMap[category.id as keyof typeof iconMap]
          return <section key={category.id} className="nav-group">
            <button className="nav-category" aria-expanded={categoryOpen} onClick={() => toggleExpanded(category.id)}><span className="nav-category-label"><CategoryIcon size={15}/>{category.title[locale]}</span><ChevronDown size={14}/></button>
            {categoryOpen && <div className="nav-category-children">{visibleSections.map((section) => {
              const key = `${category.id}:${section.id}`
              const open = expandedGroups.has(key)
              return <details key={key} className="nav-subgroup" open={open}>
                <summary onClick={(event) => { event.preventDefault(); toggleExpanded(key) }} aria-expanded={open}><span>{section.title[locale]}</span><ChevronDown size={13}/></summary>
                <div className="nav-group-pages">{section.pages.map((item) => <button key={item.id} className={`nav-link ${item.id === page.id ? 'active' : ''}`} onClick={() => navigate(item)} aria-current={item.id === page.id ? 'page' : undefined}><span className="nav-marker"/><span className="nav-link-text">{item.title[locale]}</span>{item.id === page.id && <ChevronLeft size={14}/>}</button>)}</div>
              </details>
            })}</div>}
          </section>
        })}
        {!visiblePages.length && <p className="empty-search">{texts.empty}</p>}
      </nav>
      <div className="sidebar-bottom"><p>{texts.disclaimer}</p></div>
    </>
  }

  const index = readingOrder.findIndex((item) => item.id === page.id)
  const { previous, next } = pageNeighbors(page.id)
  const trail = pageTrail(page.id)
  const groupsList = groups[locale]
  const rankedSearchResults = useMemo(() => {
    const term = normalizeSearch(deferredQuery)
    if (!term || !searchIndex) return []
    return searchIndex.map((record) => {
      const doc = pages.find((page) => page.id === record.id)
      const title = normalizeSearch(record.title), description = normalizeSearch(record.description), content = normalizeSearch(record.text)
      const category = normalizeSearch(doc ? groups[locale][doc.group] : '')
      const heading = record.headings.find((item) => normalizeSearch(item.text).includes(term))
      const contentIndex = content.indexOf(term)
      const excerpt = contentIndex >= 0 ? record.text.slice(Math.max(0, contentIndex - 72), contentIndex + term.length + 110) : record.description
      const score = title === term ? 100 : title.includes(term) ? 60 : heading ? 40 : category.includes(term) ? 35 : description.includes(term) ? 25 : content.includes(term) ? 10 : 0
      return { ...record, heading, excerpt, score }
    }).filter((item) => item.score > 0 && pages.some((doc) => doc.id === item.id)).sort((a, b) => b.score - a.score).slice(0, 8)
  }, [deferredQuery, searchIndex, locale])



  return <div className="docs-app" dir={isFa ? 'rtl' : 'ltr'}>
    <a className="skip-link" href="#article">{isFa ? 'رفتن به محتوای اصلی' : 'Skip to content'}</a>
    <header className="topbar" id="top">
      <div className="topbar-brand"><button className="mobile-menu" onClick={() => setMobileNav(true)} aria-label={texts.menu}><Menu size={19}/></button><a href="?page=overview" onClick={(event) => { event.preventDefault(); navigate(pages[0]) }} className="brand-lockup"><span className="brand-mark">A</span><span><strong className="brand-name">ARG</strong><small>{texts.title}</small></span></a><span className="top-divider"/><Badge>v{ARG_VERSION}</Badge></div>
      <div className="topbar-actions"><label className="top-search"><Search size={16}/><input ref={searchRef} aria-label={texts.search} placeholder={texts.search} value={query} onChange={(event) => setQuery(event.target.value)}/><kbd>/</kbd></label><Button variant="outline" size="sm" onClick={() => setLanguage(isFa ? 'en' : 'fa')} aria-label="Change language"><Globe2 size={15}/>{texts.language}</Button><Button variant="ghost" size="icon" onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')} aria-label={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}>{theme === 'dark' ? <Sun size={17}/> : <Moon size={17}/>}</Button><a className="icon-link" href="https://github.com/themrcortex/arg" target="_blank" rel="noreferrer" aria-label={texts.github}><Github size={17}/></a></div>
    </header>
    {query.trim().length >= 2 && !mobileNav && <div className="search-results" role="region" aria-label={isFa ? 'نتایج جست‌وجو' : 'Search results'}>{!searchIndex ? <p>{isFa ? 'در حال آماده‌سازی جست‌وجو…' : 'Preparing search…'}</p> : rankedSearchResults.length ? rankedSearchResults.map((result) => {
      const target = pages.find((item) => item.id === result.id)!
      return <button key={result.id} onClick={() => navigate(target, result.heading?.id)}><span className="search-result-category">{groupsList[target.group]}</span><strong>{highlightText(target.title[locale], query.trim())}</strong><small>{highlightText(result.heading?.text || result.excerpt || target.summary[locale], query.trim())}</small></button>
    }) : <p>{isFa ? 'نتیجه‌ای پیدا نشد.' : 'No results found.'}</p>}</div>}
    <div className={`page-progress ${pageLoading || articleLoading ? 'is-active' : ''}`} role="status" aria-live="polite" aria-label={pageLoading || articleLoading ? texts.loadingPage : undefined}><span/></div>
    <div className="docs-layout">
      <aside className="desktop-sidebar">{renderSidebar()}</aside>
      <main id="article" className={`article-column ${pageLoading ? 'is-transitioning' : ''}`} key={`${locale}-${page.id}`}>
        <div className="breadcrumbs"><span>{trail[0]?.title[locale] || groupsList[page.group]}</span><ChevronLeft size={13}/><span>{trail[1]?.title[locale] || page.title[locale]}</span><ChevronLeft size={13}/><span>{page.title[locale]}</span></div>
        {page.id === 'overview' && <section className="hero-card"><div className="hero-grid"/><div className="hero-copy"><Badge><span className="status-dot"/>{isFa ? 'راهنمای رسمی' : 'OFFICIAL DOCUMENTATION'}</Badge><h1>{isFa ? 'سرور را روشن ببین.' : 'See your server clearly.'}<br/><span>{isFa ? 'با ARG.' : 'With ARG.'}</span></h1><p>{isFa ? 'راهنمای عملی و دقیق برای نصب، ایمن‌سازی و مدیریت سرور لینوکسی با ARG.' : 'A practical, detailed guide to installing, securing, and operating a Linux server with ARG.'}</p><div className="hero-actions"><Button onClick={() => navigate(pages[1])}>{texts.start}<ArrowDown size={15}/></Button><a href="https://github.com/themrcortex/arg" target="_blank" rel="noreferrer" className="hero-github"><Github size={15}/>{texts.github}<ExternalLink size={12}/></a></div></div><div className="hero-art" aria-hidden="true"><div className="orbit orbit-one"/><div className="orbit orbit-two"/><div className="server-glyph"><Server size={54} strokeWidth={1.2}/><i/><i/></div><div className="hero-art-label">YOUR SERVER · CLEARLY</div></div><div className="hero-meta"><span><code>Rust</code><small>{isFa ? 'Backend' : 'Backend'}</small></span><span><code>React</code><small>Web panel</small></span><span><code>systemd</code><small>{isFa ? 'سرویس' : 'Service'}</small></span></div></section>}
        <div className="article-heading"><div><p className="eyebrow"><span className="eyebrow-dot"/>{trail[0]?.title[locale] || groupsList[page.group]}</p><h1>{page.title[locale]}</h1><p className="article-summary">{page.summary[locale]}</p></div><span className="article-index">{String(index + 1).padStart(2, '0')} <i>/</i> {String(pages.length).padStart(2, '0')}</span></div>
        {article.toc.length > 0 && <details className="mobile-toc"><summary>{texts.toc}<ChevronLeft size={14}/></summary><nav>{article.toc.map((item) => <a key={item.id} className={item.depth === 3 ? 'toc-sub' : ''} href={`#${item.id}`} onClick={() => setMobileNav(false)}>{item.text}</a>)}</nav></details>}
        {articleLoading ? <section className="article-skeleton" role="status" aria-label={texts.loadingPage}><span className="skeleton-line w-4/5"/><span className="skeleton-line w-full"/><span className="skeleton-line w-11/12"/><span className="skeleton-line w-3/5"/><div className="skeleton-block"/><span className="skeleton-line w-full"/><span className="skeleton-line w-5/6"/></section> : articleError ? <section className="article-load-error" role="alert"><p>{texts.articleError}</p><Button variant="outline" size="sm" onClick={() => setArticleReload((value) => value + 1)}>{texts.retry}</Button></section> : <div className="article-content" dangerouslySetInnerHTML={{ __html: article.html }} onClick={async (event) => {
          const target = event.target as HTMLElement
          const pageLink = target.closest<HTMLAnchorElement>('a[href^="?page="]')
          if (pageLink) {
            const targetId = new URL(pageLink.href).searchParams.get('page')
            const targetPage = pages.find((item) => item.id === targetId)
            if (targetPage) {
              event.preventDefault()
              navigate(targetPage)
              return
            }
          }
          const copyButton = target.closest<HTMLButtonElement>('[data-copy-code]')
          if (copyButton) {
            const code = copyButton.closest('.doc-code')?.querySelector('code')?.textContent || ''
            const ok = await copyText(code)
            if (ok) { copyButton.textContent = texts.copied; setTimeout(() => { copyButton.textContent = isFa ? 'کپی' : 'Copy' }, 1300) }
          }
        }}/>}
        <nav className="page-pager" aria-label={isFa ? 'ناوبری صفحات' : 'Page navigation'}>
          {previous ? <button onClick={() => navigate(previous)}><span><ArrowRight size={15}/>{texts.previous}</span><strong>{previous.title[locale]}</strong></button> : <span/>}
          {next ? <button className="pager-next" onClick={() => navigate(next)}><span>{texts.next}<ArrowLeft size={15}/></span><strong>{next.title[locale]}</strong></button> : <span/>}
        </nav>
        <footer className="article-footer"><span>ARG · VPS CONTROL CENTER</span><span>{isFa ? `مستندات متن‌باز · نسخهٔ ${ARG_VERSION}` : `Open-source docs · v${ARG_VERSION}`}</span></footer>
      </main>
      <aside className="toc-column"><div className="toc-sticky"><h2>{texts.toc}</h2><div className="reading-progress" aria-label={isFa ? `پیشرفت خواندن ${readingProgress} درصد` : `Reading progress ${readingProgress}%`}><span style={{ width: `${readingProgress}%` }}/></div>{article.toc.length ? <nav>{article.toc.map((item) => <a key={item.id} className={`${item.depth === 3 ? 'toc-sub ' : ''}${activeHeading === item.id ? 'toc-active' : ''}`} href={`#${item.id}`}>{item.text}</a>)}</nav> : <p>{isFa ? 'این صفحه سرفصل فرعی ندارد.' : 'No subsections on this page.'}</p>}<Card className="help-card"><CardHeader><div className="help-icon"><Command size={16}/></div><CardTitle>{isFa ? 'سریع به نتیجه برس' : 'Find your way'}</CardTitle></CardHeader><CardContent><p>{isFa ? 'از / یا Ctrl/⌘ K برای جست‌وجوی متن راهنما استفاده کن.' : 'Use / or Ctrl/⌘ K to search the full guide text.'}</p><a href="https://github.com/themrcortex/arg/issues" target="_blank" rel="noreferrer">{texts.support}<ExternalLink size={12}/></a></CardContent></Card></div></aside>
    </div>
    {mobileNav && <div className="mobile-overlay" role="presentation" onPointerDown={(event) => { if (event.target === event.currentTarget) setMobileNav(false) }}><aside className="mobile-sidebar" role="dialog" aria-modal="true" aria-label={texts.menu} onKeyDown={(event) => {
      if (event.key !== 'Tab') return
      const focusable = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), a[href], summary'))
      if (!focusable.length) return
      const first = focusable[0], last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
    }}>{renderSidebar(true)}</aside></div>}
    {showBackTop && <button type="button" className="back-top is-visible" onClick={() => window.scrollTo({ top: 0, behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' })} aria-label={texts.top} title={texts.top}><ArrowUp size={16}/></button>}
  </div>
}
