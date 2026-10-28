import { useDeferredValue, useEffect, useMemo, useRef, useState } from 'react'
import DOMPurify from 'dompurify'
import { marked } from 'marked'
import { Activity, ArrowDown, ArrowLeft, ArrowRight, ArrowUp, BookOpen, Boxes, ChevronLeft, ChevronRight, Code2, Command, Container, Database, ExternalLink, Github, Globe2, Menu, Moon, Network, Search, Settings2, Shield, Sun, X } from 'lucide-react'
import { groups, isMarkdownPageLink, navigation, pages, pageNeighbors, pageTrail, readingOrder, resolveMarkdownPageLink, resolvePageId, type DocPage, type Locale } from './catalog'
import { Badge } from './components/ui/badge'
import { ARG_VERSION } from './version'
import { Button } from './components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from './components/ui/card'

const sourceFor = async (locale: Locale, file: string) => {
  const response = await fetch(`${import.meta.env.BASE_URL}content/${locale}/${encodeURIComponent(file)}.md`)
  if (!response.ok) throw new Error(`Documentation article not found: ${locale}/${file}`)
  return response.text()
}
const getInitialPage = () => {
  const requested = new URLSearchParams(window.location.search).get('page')
  return requested ? resolvePageId(requested) || 'overview' : 'overview'
}
const getInitialLocale = (): Locale => {
  const requested = new URLSearchParams(window.location.search).get('lang')
  if (requested === 'fa' || requested === 'en') return requested
  return localStorage.getItem('arg-docs-locale') === 'en' ? 'en' : 'fa'
}
const getInitialCategory = () => pages.find((page) => page.id === getInitialPage())?.group || navigation[0]?.id || 'gettingStarted'
const normalizeLegacyPageUrl = () => {
  const url = new URL(window.location.href)
  const requested = url.searchParams.get('page')
  const resolved = requested ? resolvePageId(requested) : undefined
  if (!requested || !resolved || requested === resolved) return
  url.searchParams.set('page', resolved)
  window.history.replaceState(window.history.state, '', `${url.pathname}${url.search}${url.hash}`)
}

const slugHeading = (text: string) => text.toLocaleLowerCase().normalize('NFKD').replace(/[\u0300-\u036f]/g, '').replace(/[^\p{L}\p{N}]+/gu, '-').replace(/^-|-$/g, '') || 'section'

function parseArticle(source: string, locale: Locale, pageTitle: string) {
  const rendered = String(marked.parse(source, { gfm: true, breaks: false }))
  const doc = new DOMParser().parseFromString(rendered, 'text/html')
  const firstTitle = doc.body.querySelector('h1')
  if (firstTitle?.textContent?.trim() === pageTitle.trim()) firstTitle.remove()
  doc.body.querySelectorAll('h1').forEach((heading) => {
    const replacement = doc.createElement('h2')
    replacement.innerHTML = heading.innerHTML
    heading.replaceWith(replacement)
  })
  const seen = new Map<string, number>()
  const toc = Array.from(doc.body.querySelectorAll('h2,h3,h4,h5,h6')).map((heading) => {
    const text = heading.textContent?.trim() || ''
    const slug = slugHeading(text)
    const count = seen.get(slug) || 0
    seen.set(slug, count + 1)
    const id = count ? `${slug}-${count + 1}` : slug
    heading.id = id
    const permalink = doc.createElement('a')
    permalink.className = 'heading-anchor'
    permalink.href = `#${id}`
    permalink.setAttribute('aria-label', locale === 'fa' ? `پیوند مستقیم به تیتر ${text}` : `Permalink to ${text}`)
    permalink.title = locale === 'fa' ? 'پیوند مستقیم به این تیتر' : 'Permalink to this heading'
    permalink.textContent = '¶'
    heading.append(permalink)
    const depth = Number(heading.tagName.slice(1))
    return depth <= 3 ? { id, text, depth } : null
  }).filter((item): item is { id: string; text: string; depth: number } => item !== null)
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
  doc.body.querySelectorAll('a[href]').forEach((link) => {
    const href = link.getAttribute('href') || ''
    if (!isMarkdownPageLink(href)) return
    const resolved = resolveMarkdownPageLink(href, locale)
    if (!resolved) {
      link.removeAttribute('href')
      link.setAttribute('aria-disabled', 'true')
      link.classList.add('unresolved-doc-link')
      return
    }
    link.setAttribute('href', `?page=${encodeURIComponent(resolved.id)}&lang=${resolved.locale}${resolved.anchor ? `#${encodeURIComponent(resolved.anchor)}` : ''}`)
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
const categoryIcons = { gettingStarted: BookOpen, dashboard: Activity, network: Network, docker: Container, data: Database, applications: Boxes, system: Settings2, security: Shield, reference: Code2 }

export default function App() {
  const [locale, setLocale] = useState<Locale>(getInitialLocale)
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
  const [selectedCategory, setSelectedCategory] = useState<string>(getInitialCategory)
  const searchRef = useRef<HTMLInputElement>(null)
  const mobileSearchRef = useRef<HTMLInputElement>(null)
  const previousFocus = useRef<HTMLElement | null>(null)
  const transitionTimer = useRef<number | null>(null)
  const previousPageId = useRef(pageId)
  const deferredQuery = useDeferredValue(query)
  const page = pages.find((item) => item.id === pageId) || pages[0]
  const article = useMemo(() => parseArticle(articleSource, locale, page.title[locale]), [articleSource, locale, page.title, page.id])
  const isFa = locale === 'fa'
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
    normalizeLegacyPageUrl()
  }, [])

  useEffect(() => {
    const onPop = () => {
      normalizeLegacyPageUrl()
      startPageTransition()
      setPageId(getInitialPage())
      const nextLocale = getInitialLocale()
      if (nextLocale !== locale) setSearchIndex(null)
      setLocale(nextLocale)
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
  }, [locale])

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
    const [category] = pageTrail(page.id)
    const categoryId = category?.id || page.group
    setSelectedCategory(categoryId)
    requestAnimationFrame(() => {
      const sidebar = mobileNav ? '.mobile-sidebar' : '.desktop-sidebar'
      document.querySelector(`${sidebar} .nav-link.active`)?.scrollIntoView({ block: 'nearest' })
    })
  }, [page.group, page.id, mobileNav])

  useEffect(() => { localStorage.setItem('arg-docs-category', selectedCategory) }, [selectedCategory])

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

  function navigate(item: DocPage, anchor?: string, targetLocale: Locale = locale) {
    if (item.id !== page.id) startPageTransition()
    setPageId(item.id)
    if (targetLocale !== locale) setSearchIndex(null)
    setLocale(targetLocale)
    setTargetAnchor(anchor || '')
    setQuery('')
    setMobileNav(false)
    const url = new URL(window.location.href)
    url.searchParams.set('page', item.id)
    url.searchParams.set('lang', targetLocale)
    url.hash = anchor ? encodeURIComponent(anchor) : ''
    history.pushState(null, '', `${url.pathname}${url.search}${url.hash}`)
  }

  function setLanguage(next: Locale) {
    startPageTransition()
    setLocale(next)
    setSearchIndex(null)
    setTargetAnchor(decodeURIComponent(window.location.hash.slice(1)))
    setQuery('')
    const url = new URL(window.location.href)
    url.searchParams.set('page', page.id)
    url.searchParams.set('lang', next)
    history.pushState(null, '', `${url.pathname}${url.search}${url.hash}`)
  }

  function startPageTransition() {
    setPageLoading(true)
    if (transitionTimer.current !== null) window.clearTimeout(transitionTimer.current)
    transitionTimer.current = window.setTimeout(() => {
      setPageLoading(false)
      transitionTimer.current = null
    }, 420)
  }

  function selectCategory(id: string) {
    setSelectedCategory(id)
    setQuery('')
  }

  function focusDocsSearch() {
    if (window.matchMedia('(max-width: 760px)').matches) {
      setMobileNav(true)
      requestAnimationFrame(() => mobileSearchRef.current?.focus())
    } else searchRef.current?.focus()
  }

  function renderSidebar(mobile = false) {
    const idPrefix = mobile ? 'mobile-' : 'desktop-'
    const searching = query.trim().length >= 2
    const activeTrail = pageTrail(page.id)
    const activeSection = activeTrail[1]?.id
    const currentCategory = navigation.find((category) => category.id === selectedCategory) || navigation[0]
    const currentCategoryPages = currentCategory?.children.flatMap((section) => section.pages) || []
    return <>
      <div className="sidebar-heading"><div className="sidebar-heading-icon"><BookOpen size={17}/></div><div className="sidebar-heading-copy min-w-0"><p>{isFa ? 'راهنمای ARG' : 'ARG docs'}</p><span>{texts.menu} · {readingOrder.length} {texts.allDocs}</span></div><button className="mobile-close" onClick={() => setMobileNav(false)} aria-label={texts.close}><X size={18}/></button></div>
      <label className="side-search"><Search size={15}/><input ref={mobile ? mobileSearchRef : undefined} aria-label={texts.search} placeholder={texts.search} value={query} onChange={(event) => setQuery(event.target.value)}/><kbd>/</kbd></label>
      <nav className="side-nav sidebar-two-stage" aria-label={texts.menu}>
        <div className="sidebar-category-rail" role="group" aria-label={isFa ? 'انتخاب دسته‌بندی' : 'Choose a category'}>
          {navigation.map((category) => {
            const Icon = categoryIcons[category.id as keyof typeof categoryIcons] || BookOpen
            const count = category.children.reduce((total, section) => total + section.pages.length, 0)
            return <button key={category.id} className={`sidebar-category-option ${selectedCategory === category.id ? 'is-selected' : ''}`} type="button" aria-pressed={selectedCategory === category.id} onClick={() => selectCategory(category.id)}>
              <Icon size={15}/><span>{category.title[locale]}</span><small>{count}</small>
            </button>
          })}
        </div>
        <div className="sidebar-panel" aria-live="polite">
          {!searching && currentCategory && <div className="sidebar-panel-heading"><strong>{currentCategory.title[locale]}</strong><span>{currentCategoryPages.length} {texts.allDocs}</span></div>}
          {searching && (searchIndex ? rankedSearchResults.length ? <div className="inline-search-results">{rankedSearchResults.map((result) => {
          const target = pages.find((item) => item.id === result.id)!
          return <button key={result.id} onClick={() => navigate(target, result.heading?.id)}><small>{groupsList[target.group]}</small><strong>{highlightText(target.title[locale], query.trim())}</strong><span>{highlightText(result.heading?.text || result.excerpt || target.summary[locale], query.trim())}</span></button>
        })}</div> : <p className="empty-search">{isFa ? 'نتیجه‌ای پیدا نشد.' : 'No results found.'}</p> : <p className="empty-search">{isFa ? 'در حال آماده‌سازی جست‌وجو…' : 'Preparing search…'}</p>)}
          {!searching && currentCategory?.children.map((section) => {
              const sectionIsCurrent = currentCategory.id === page.group && activeSection === section.id
              const sectionContentId = `${idPrefix}section-${currentCategory.id}-${section.id}`
              return <section key={section.id} className={`nav-subgroup ${sectionIsCurrent ? 'is-current' : ''}`}>
                <h3 className="sidebar-section-title" id={sectionContentId}>{section.title[locale]}</h3>
                <div className="nav-group-pages" aria-labelledby={sectionContentId}>{section.pages.map((item) => <a key={item.id} href={`?page=${item.id}&lang=${locale}`} className={`nav-link ${item.id === page.id ? 'active' : ''}`} onClick={(event) => { if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return; event.preventDefault(); navigate(item) }} aria-current={item.id === page.id ? 'page' : undefined}><span className="nav-marker"/><span className="nav-link-text">{item.title[locale]}</span>{item.id === page.id && <ChevronLeft size={14}/>}</a>)}</div>
              </section>
            })}
        </div>
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

  const homeSteps = ['install', 'accounts-settings', 'dashboard'].map((id) => pages.find((item) => item.id === id)).filter((item): item is DocPage => !!item)



  return <div className="docs-app" dir={isFa ? 'rtl' : 'ltr'}>
    <a className="skip-link" href="#article">{isFa ? 'رفتن به محتوای اصلی' : 'Skip to content'}</a>
    <header className="topbar" id="top">
      <div className="topbar-brand"><button className="mobile-menu" onClick={() => setMobileNav(true)} aria-label={texts.menu} aria-haspopup="dialog" aria-expanded={mobileNav}><Menu size={19}/></button><a href="?page=overview" onClick={(event) => { event.preventDefault(); navigate(pages[0]) }} className="brand-lockup"><span className="brand-mark">A</span><span><strong className="brand-name">ARG</strong><small>{texts.title}</small></span></a><span className="top-divider"/><Badge>v{ARG_VERSION}</Badge></div>
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
        <nav className="breadcrumbs" aria-label={isFa ? 'مسیر صفحه' : 'Breadcrumb'} dir={isFa ? 'rtl' : 'ltr'}><span>{trail[0]?.title[locale] || groupsList[page.group]}</span>{isFa ? <ChevronLeft size={13}/> : <ChevronRight size={13}/>}<span>{trail[1]?.title[locale] || page.title[locale]}</span>{isFa ? <ChevronLeft size={13}/> : <ChevronRight size={13}/>}<span aria-current="page">{page.title[locale]}</span></nav>
        {page.id === 'overview' && <section className="hero-card" aria-labelledby="home-title">
          <div className="hero-copy">
            <p className="hero-kicker"><span className="hero-kicker-mark"><BookOpen size={14}/></span>{isFa ? 'مستندات رسمی ARG' : 'ARG DOCUMENTATION'}</p>
            <h1 id="home-title">{isFa ? <>مدیریت VPS،<br/><span>ساده و روشن.</span></> : <>A clearer way<br/><span>to manage your VPS.</span></>}<a className="title-anchor" href="#home-title" aria-label={isFa ? 'پیوند مستقیم به تیتر صفحه' : 'Permalink to page title'} title={isFa ? 'پیوند مستقیم به این تیتر' : 'Permalink to this heading'}>¶</a></h1>
            <p>{isFa ? 'راهنمای دوزبانه برای نصب، ایمن‌سازی و مدیریت VPS لینوکسی با ARG؛ بر پایهٔ قابلیت‌های موجود در همین نسخه.' : 'A bilingual guide to installing, securing, and managing a Linux VPS with ARG, grounded in the features available in this release.'}</p>
            <div className="hero-actions"><Button onClick={() => { const install = pages.find((item) => item.id === 'install'); if (install) navigate(install) }}>{texts.start}<ArrowDown size={15}/></Button><Button variant="outline" onClick={focusDocsSearch}><Search size={15}/>{isFa ? 'جست‌وجو در راهنماها' : 'Search the docs'}<kbd>/</kbd></Button></div>
            <div className="hero-facts"><span><code>Rust</code>{isFa ? 'هستهٔ سرور' : 'server backend'}</span><span><code>React</code>{isFa ? 'پنل وب' : 'web panel'}</span><span><code>systemd</code>{isFa ? 'مدیریت سرویس' : 'service manager'}</span></div>
          </div>
          <div className="hero-start"><p className="hero-start-heading">{isFa ? 'از اینجا شروع کنید' : 'A good place to start'}<span>{isFa ? 'راهنماهای پیشنهادی' : 'Recommended guides'}</span></p>
            <div className="hero-start-list">{homeSteps.map((item, step) => <a key={item.id} href={`?page=${item.id}`} className="hero-start-link" onClick={(event) => { if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return; event.preventDefault(); navigate(item) }}><span className="hero-start-number">{String(step + 1).padStart(2, '0')}</span><span className="hero-start-copy"><strong>{item.title[locale]}</strong><small>{item.summary[locale]}</small></span>{isFa ? <ArrowLeft className="hero-start-arrow" size={16}/> : <ArrowRight className="hero-start-arrow" size={16}/>}</a>)}</div>
            <a className="hero-github" href="https://github.com/themrcortex/arg" target="_blank" rel="noreferrer"><Github size={15}/>{texts.github}<ExternalLink size={12}/></a>
          </div>
        </section>}
        {page.id !== 'overview' && <div className="article-heading"><div><p className="eyebrow"><span className="eyebrow-dot"/>{trail[0]?.title[locale] || groupsList[page.group]}</p><h1 id="article-title">{page.title[locale]}<a className="title-anchor" href="#article-title" aria-label={isFa ? 'پیوند مستقیم به تیتر صفحه' : 'Permalink to page title'} title={isFa ? 'پیوند مستقیم به این تیتر' : 'Permalink to this heading'}>¶</a></h1><p className="article-summary">{page.summary[locale]}</p></div><span className="article-index">{String(index + 1).padStart(2, '0')} <i>/</i> {String(pages.length).padStart(2, '0')}</span></div>}
        {article.toc.length > 0 && <details className="mobile-toc"><summary>{texts.toc}<ChevronLeft size={14}/></summary><nav>{article.toc.map((item) => <a key={item.id} className={item.depth === 3 ? 'toc-sub' : ''} href={`#${item.id}`} onClick={() => setMobileNav(false)}>{item.text}</a>)}</nav></details>}
        {articleLoading ? <section className="article-skeleton" role="status" aria-label={texts.loadingPage}><span className="skeleton-line w-4/5"/><span className="skeleton-line w-full"/><span className="skeleton-line w-11/12"/><span className="skeleton-line w-3/5"/><div className="skeleton-block"/><span className="skeleton-line w-full"/><span className="skeleton-line w-5/6"/></section> : articleError ? <section className="article-load-error" role="alert"><p>{texts.articleError}</p><Button variant="outline" size="sm" onClick={() => setArticleReload((value) => value + 1)}>{texts.retry}</Button></section> : <div className="article-content" dangerouslySetInnerHTML={{ __html: article.html }} onClick={async (event) => {
          const target = event.target as HTMLElement
          const pageLink = target.closest<HTMLAnchorElement>('a[href*="page="]')
          if (pageLink) {
            const url = new URL(pageLink.href, window.location.href)
            const targetId = url.searchParams.get('page')
            const resolvedId = targetId ? resolvePageId(targetId) : undefined
            const targetPage = pages.find((item) => item.id === resolvedId)
            if (targetPage) {
              event.preventDefault()
              const requestedLocale = url.searchParams.get('lang')
              const targetLocale = requestedLocale === 'fa' || requestedLocale === 'en' ? requestedLocale : locale
              navigate(targetPage, decodeURIComponent(url.hash.slice(1)), targetLocale)
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
          <button className={!previous ? 'is-disabled' : ''} disabled={!previous} onClick={() => previous && navigate(previous)}><span>{isFa ? <ArrowRight size={15} /> : <ArrowLeft size={15} />}{texts.previous}</span><strong>{previous?.title[locale] || (isFa ? 'ابتدای مستندات' : 'Start of documentation')}</strong></button>
          <button className={`pager-next ${!next ? 'is-disabled' : ''}`} disabled={!next} onClick={() => next && navigate(next)}><span>{texts.next}{isFa ? <ArrowLeft size={15} /> : <ArrowRight size={15} />}</span><strong>{next?.title[locale] || (isFa ? 'پایان مستندات' : 'End of documentation')}</strong></button>
        </nav>
        <footer className="article-footer"><span>ARG · VPS CONTROL CENTER</span><span>{isFa ? `مستندات متن‌باز · نسخهٔ ${ARG_VERSION}` : `Open-source docs · v${ARG_VERSION}`}</span></footer>
      </main>
      <aside className="toc-column"><div className="toc-sticky"><h2>{texts.toc}</h2><div className="reading-progress" aria-label={isFa ? `پیشرفت خواندن ${readingProgress} درصد` : `Reading progress ${readingProgress}%`}><span style={{ width: `${readingProgress}%` }}/></div>{article.toc.length ? <nav>{article.toc.map((item) => <a key={item.id} className={`${item.depth === 3 ? 'toc-sub ' : ''}${activeHeading === item.id ? 'toc-active' : ''}`} href={`#${item.id}`}>{item.text}</a>)}</nav> : <p>{isFa ? 'این صفحه سرفصل فرعی ندارد.' : 'No subsections on this page.'}</p>}<Card className="help-card"><CardHeader><div className="help-icon"><Command size={16}/></div><CardTitle>{isFa ? 'سریع به نتیجه برس' : 'Find your way'}</CardTitle></CardHeader><CardContent><p>{isFa ? 'از / یا Ctrl/⌘ K برای جست‌وجوی متن راهنما استفاده کن.' : 'Use / or Ctrl/⌘ K to search the full guide text.'}</p><a href="https://github.com/themrcortex/arg/issues" target="_blank" rel="noreferrer">{texts.support}<ExternalLink size={12}/></a></CardContent></Card></div></aside>
    </div>
    {mobileNav && <div className="mobile-overlay" role="presentation" onPointerDown={(event) => { if (event.target === event.currentTarget) setMobileNav(false) }}><aside className="mobile-sidebar" role="dialog" aria-modal="true" aria-label={texts.menu} onKeyDown={(event) => {
      if (event.key !== 'Tab') return
      const focusable = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), a[href], summary')).filter((element) => element.getClientRects().length > 0)
      if (!focusable.length) return
      const first = focusable[0], last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
    }}>{renderSidebar(true)}</aside></div>}
    {showBackTop && <button type="button" className="back-top is-visible" onClick={() => window.scrollTo({ top: 0, behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' })} aria-label={texts.top} title={texts.top}><ArrowUp size={16}/></button>}
  </div>
}
