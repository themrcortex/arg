import { useCallback, useEffect, useRef, useState } from 'react'
import { FitAddon } from '@xterm/addon-fit'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { Check, Circle, Clipboard, ClipboardPaste, Copy, LoaderCircle, Maximize2, Minimize2, Pencil, Plus, RotateCw, Scissors, ShieldAlert, TerminalSquare, Trash2, X } from 'lucide-react'
import { Alert, AlertDescription } from './components/ui/alert'
import { Button } from './components/ui/button'
import { Badge } from './components/ui/badge'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from './components/ui/tooltip'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'
import { Input } from './components/ui/input'
import { api } from './lib/api'

type Connection = 'connecting' | 'connected' | 'disconnected' | 'error'
type TerminalTab = { id: string; name: string; mode: 'user' | 'root' }

export function TerminalPage({ Heading }: { Heading: (props: { title: string; description: string }) => React.ReactNode }) {
  const [tabs, setTabs] = useState<TerminalTab[]>([{ id: crypto.randomUUID(), name: 'Terminal 1', mode: 'user' }])
  const [activeId, setActiveId] = useState(tabs[0].id)
  const [renameId, setRenameId] = useState<string | null>(null)
  const [clipboardNote, setClipboardNote] = useState('')
  const [rootPromptOpen, setRootPromptOpen] = useState(false)
  const [rootPassword, setRootPassword] = useState('')
  const [rootAuthorizing, setRootAuthorizing] = useState(false)
  const [rootError, setRootError] = useState('')

  const addTab = () => {
    let index = 1
    while (tabs.some((tab) => tab.name === `Terminal ${index}`)) index += 1
    const next = { id: crypto.randomUUID(), name: `Terminal ${index}`, mode: 'user' as const }
    setTabs((current) => [...current, next])
    setActiveId(next.id)
  }
  const addRootTab = async () => {
    setRootAuthorizing(true)
    setRootError('')
    try {
      await api('terminal/root/authorize', { method: 'POST', body: JSON.stringify({ password: rootPassword }) })
    } catch (error) {
      setRootError(error instanceof Error ? error.message : 'تأیید هویت ناموفق بود.')
      setRootAuthorizing(false)
      return
    }
    let index = 1
    let name = 'Terminal Root'
    while (tabs.some((tab) => tab.name === name)) { index += 1; name = `Terminal Root ${index}` }
    const next = { id: crypto.randomUUID(), name, mode: 'root' as const }
    setTabs((current) => [...current, next])
    setActiveId(next.id)
    setRootPromptOpen(false)
    setRootPassword('')
    setRootAuthorizing(false)
    setClipboardNote('')
  }
  const closeTab = (id: string) => {
    const remaining = tabs.filter((tab) => tab.id !== id)
    setTabs(remaining)
    if (activeId === id) setActiveId(remaining.length ? remaining[Math.max(0, tabs.findIndex((tab) => tab.id === id) - 1)].id : '')
    if (renameId === id) setRenameId(null)
  }
  const renameTab = (id: string, name: string) => {
    const clean = name.trim().slice(0, 32)
    if (clean) setTabs((current) => current.map((tab) => tab.id === id ? { ...tab, name: clean } : tab))
    setRenameId(null)
  }

  return <>
    <Heading title="ترمینال سرور" description="نشست‌های مستقل و تعاملی؛ ترمینال معمولی و روت را جداگانه باز و نام‌گذاری کن." />
    <Alert className="mb-4 border-amber-400/20 bg-amber-400/[.05]"><AlertDescription>ترمینال عادی با دسترسی محدود سرویس ARG اجرا می‌شود. برای اجرای دستورهای مدیریتی از Terminal Root استفاده کن؛ همهٔ تب‌ها نشست جداگانه دارند و سقف تعداد تب وجود ندارد.</AlertDescription></Alert>
    <TooltipProvider delayDuration={250}>
      <section className="overflow-hidden rounded-2xl border border-border bg-[#090d12] shadow-2xl shadow-black/20">
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-white/10 bg-[#111820] px-3 py-2">
          <div className="flex min-w-0 flex-1 items-center gap-2 overflow-x-auto" role="tablist" aria-label="نشست‌های ترمینال">
            {tabs.map((tab, index) => <div key={tab.id} role="presentation" className={`group flex shrink-0 items-center rounded-lg border transition-colors ${activeId === tab.id ? 'border-white/10 bg-[#090d12] text-foreground' : 'border-transparent text-muted-foreground hover:bg-white/[.04]'}`}>
              {renameId === tab.id ? <input autoFocus defaultValue={tab.name} maxLength={32} aria-label="نام تب" className="w-28 bg-transparent px-3 py-2 text-xs outline-none" onBlur={(event) => renameTab(tab.id, event.currentTarget.value)} onKeyDown={(event) => { if (event.key === 'Enter') renameTab(tab.id, event.currentTarget.value); if (event.key === 'Escape') setRenameId(null) }} /> : <button role="tab" aria-selected={activeId === tab.id} className="flex items-center gap-2 px-3 py-2 text-xs" onClick={() => { setActiveId(tab.id); setClipboardNote('') }}><TerminalSquare size={14} className={tab.mode === 'root' ? 'text-rose-300' : 'text-[#dda46f]'}/><span>{tab.name}</span><span className="font-mono text-[10px] opacity-45">{index + 1}</span></button>}
              <Tooltip><TooltipTrigger asChild><button aria-label={`تغییر نام ${tab.name}`} className="rounded p-1 opacity-0 transition-opacity hover:text-[#dda46f] focus:opacity-100 group-hover:opacity-100" onClick={() => setRenameId(tab.id)}><Pencil size={12}/></button></TooltipTrigger><TooltipContent>تغییر نام تب</TooltipContent></Tooltip>
              {tabs.length > 1 && <Tooltip><TooltipTrigger asChild><button aria-label={`بستن ${tab.name}`} className="ml-1 rounded p-1 opacity-0 transition-opacity hover:text-rose-300 focus:opacity-100 group-hover:opacity-100" onClick={() => closeTab(tab.id)}><X size={13}/></button></TooltipTrigger><TooltipContent>بستن نشست و تب</TooltipContent></Tooltip>}
            </div>)}
            <Tooltip><TooltipTrigger asChild><Button size="icon" variant="ghost" aria-label="تب جدید" onClick={addTab} className="h-8 w-8 shrink-0"><Plus size={15}/></Button></TooltipTrigger><TooltipContent>باز کردن نشست ترمینال جدید</TooltipContent></Tooltip>
          </div>
          <div className="flex shrink-0 items-center gap-2"><Button size="sm" variant="outline" className="h-8 border-rose-300/20 px-2.5 text-[11px] text-rose-100 hover:bg-rose-300/10" onClick={() => setRootPromptOpen(true)}><ShieldAlert size={14}/>Terminal Root</Button><span className="flex items-center gap-1.5 rounded-full border border-emerald-400/15 px-2.5 py-1 text-[11px] text-muted-foreground"><i className={`h-1.5 w-1.5 rounded-full ${tabs.length ? 'bg-emerald-400' : 'bg-slate-500'}`}/>{tabs.length ? `${tabs.length} نشست` : 'بدون نشست'}</span></div>
        </div>
        {tabs.length ? tabs.map((tab) => <SessionTerminal key={tab.id} tab={tab} active={activeId === tab.id} onClose={() => closeTab(tab.id)} clipboardNote={activeId === tab.id ? clipboardNote : ''} setClipboardNote={setClipboardNote} />) : <div className="grid min-h-64 place-items-center p-8 text-center"><div><div className="mx-auto grid h-12 w-12 place-items-center rounded-xl border border-[#dda46f]/20 bg-[#dda46f]/[.06] text-[#dda46f]"><TerminalSquare size={22}/></div><h3 className="mt-4 font-semibold text-foreground">همهٔ نشست‌ها بسته شده‌اند</h3><p className="mt-1 text-sm text-muted-foreground">برای شروع، یک نشست ترمینال تازه باز کن.</p><Button className="mt-4" onClick={addTab}><Plus size={15}/>ترمینال جدید</Button></div></div>}
      </section>
      <div className="mt-3 flex flex-wrap items-center justify-between gap-2 text-[11px] text-muted-foreground">
        <span className="flex items-center gap-2"><Check size={13} className="text-emerald-300"/>نشست‌ها از اتصال پنل و نشست ورود مدیر محافظت می‌شوند.</span>
        <span dir="ltr" className="font-mono">Ctrl + Shift + C copy · Ctrl + Shift + V paste</span>
      </div>
    </TooltipProvider>
    <Dialog open={rootPromptOpen} onOpenChange={(open) => { setRootPromptOpen(open); if (!open) { setRootPassword(''); setRootError('') } }}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-rose-100"><ShieldAlert size={20}/>ورود به Terminal Root</DialogTitle>
          <DialogDescription>این نشست با دسترسی کامل root روی سرور اجرا می‌شود.</DialogDescription>
        </DialogHeader>
        <div className="rounded-xl border border-rose-300/20 bg-rose-300/[.06] p-4 text-sm leading-7 text-rose-50">
          واقعاً می‌خواهی وارد ترمینال روت شوی؟ این کار خطرناک است؛ یک دستور اشتباه ممکن است داده‌ها را پاک کند، سرویس‌ها را متوقف کند یا دسترسی به سرور را از بین ببرد. فقط دستورهایی را اجرا کن که دقیقاً می‌شناسی.
        </div>
        <label className="mt-4 block space-y-2 text-sm"><span>برای تأیید، گذرواژهٔ مدیر را دوباره وارد کن</span><Input type="password" autoComplete="current-password" value={rootPassword} onChange={(event) => setRootPassword(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter' && rootPassword && !rootAuthorizing) void addRootTab() }} /></label>
        {rootError && <p role="alert" className="mt-2 text-sm text-rose-300">{rootError}</p>}
        <div className="mt-5 flex flex-wrap justify-end gap-2"><Button variant="outline" disabled={rootAuthorizing} onClick={() => setRootPromptOpen(false)}>انصراف</Button><Button variant="destructive" disabled={!rootPassword || rootAuthorizing} onClick={() => void addRootTab()}>{rootAuthorizing ? 'در حال تأیید…' : 'تأیید و بازکردن ترمینال روت'}</Button></div>
      </DialogContent>
    </Dialog>
  </>
}

function SessionTerminal({ tab, active, onClose, clipboardNote, setClipboardNote }: { tab: TerminalTab; active: boolean; onClose: () => void; clipboardNote: string; setClipboardNote: (message: string) => void }) {
  const hostRef = useRef<HTMLDivElement>(null)
  const terminalRef = useRef<Terminal | null>(null)
  const fitRef = useRef<FitAddon | null>(null)
  const socketRef = useRef<WebSocket | null>(null)
  const activeRef = useRef(active)
  activeRef.current = active
  const [connection, setConnection] = useState<Connection>('connecting')
  const [session, setSession] = useState(0)
  const [error, setError] = useState('')
  const [minimized, setMinimized] = useState(false)
  const [maximized, setMaximized] = useState(false)

  const copySelection = useCallback(async (cut = false) => {
    const terminal = terminalRef.current
    if (!terminal?.hasSelection()) { setClipboardNote('ابتدا بخشی از خروجی ترمینال را انتخاب کن.'); return }
    const text = terminal.getSelection()
    try {
      await navigator.clipboard.writeText(text)
      if (cut) terminal.clearSelection()
      setClipboardNote(cut ? 'متن انتخاب‌شده کپی و انتخاب پاک شد.' : 'متن انتخاب‌شده کپی شد.')
    } catch {
      setClipboardNote('دسترسی کلیپ‌بورد ممکن نیست؛ از منوی مرورگر کپی کن.')
    }
  }, [setClipboardNote])

  const pasteClipboard = useCallback(async () => {
    try {
      const text = await navigator.clipboard.readText()
      if (text) { terminalRef.current?.paste(text); setClipboardNote('متن در ترمینال چسبانده شد.') }
      else setClipboardNote('کلیپ‌بورد خالی است.')
    } catch {
      setClipboardNote('دسترسی کلیپ‌بورد ممکن نیست؛ از Ctrl + Shift + V مرورگر استفاده کن.')
    }
  }, [setClipboardNote])

  useEffect(() => {
    const host = hostRef.current
    if (!host) return
    const terminal = new Terminal({
      cursorBlink: true, convertEol: true, scrollback: 5000,
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace', fontSize: 13, lineHeight: 1.25,
      theme: { background: '#090d12', foreground: '#e8edf2', cursor: '#dda46f', cursorAccent: '#090d12', selectionBackground: '#dda46f55', black: '#111820', red: '#e87575', green: '#53c992', yellow: '#dda46f', blue: '#72aee6', magenta: '#b892e8', cyan: '#58c4c0', white: '#e8edf2', brightBlack: '#63717d', brightRed: '#ff9292', brightGreen: '#70e0a9', brightYellow: '#f0bf85', brightBlue: '#8bc1f5', brightMagenta: '#c7a8f0', brightCyan: '#71d8d3', brightWhite: '#ffffff' },
    })
    const fit = new FitAddon()
    terminal.loadAddon(fit)
    terminal.open(host)
    terminalRef.current = terminal
    fitRef.current = fit
    terminal.writeln(tab.mode === 'root' ? '\x1b[1;31mARG TERMINAL ROOT — FULL PRIVILEGE\x1b[0m' : '\x1b[38;2;221;164;111mARG SERVER TERMINAL\x1b[0m')
    terminal.writeln(`Session: ${tab.name} · User: ${tab.mode === 'root' ? 'root' : 'arg'} · Starting secure PTY…\r\n`)

    const scheme = location.protocol === 'https:' ? 'wss:' : 'ws:'
    let socket: WebSocket
    try {
      socket = new WebSocket(`${scheme}//${location.host}/api/terminal${tab.mode === 'root' ? '?mode=root' : ''}`)
      socket.binaryType = 'arraybuffer'
      socketRef.current = socket
    } catch {
      setConnection('error')
      setError('مرورگر نتوانست اتصال ترمینال را آغاز کند. دوباره تلاش کن.')
      return () => terminal.dispose()
    }
    let disposed = false
    let openTimer: number | undefined
    socket.onopen = () => {
      if (openTimer) window.clearTimeout(openTimer)
      setConnection('connected'); setError('')
      requestAnimationFrame(() => {
        try { if (host.getClientRects().length) fit.fit(); socket.send(JSON.stringify({ type: 'resize', cols: terminal.cols, rows: terminal.rows })) } catch { /* Hidden tabs resize when selected. */ }
        if (activeRef.current) terminal.focus()
      })
    }
    socket.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) terminal.write(new Uint8Array(event.data))
      else if (typeof event.data === 'string') terminal.write(event.data)
    }
    socket.onerror = () => { /* Wait for close: browsers emit error during ordinary reconnect handshakes too. */ }
    socket.onclose = () => {
      if (openTimer) window.clearTimeout(openTimer)
      if (!disposed) {
        setConnection('error')
        setError('اتصال ترمینال برقرار نشد. وضعیت سرویس و دسترسی حساب را بررسی و دوباره وصل شو.')
      }
    }
    openTimer = window.setTimeout(() => {
      if (socket.readyState === WebSocket.CONNECTING) terminal.write('\r\n\x1b[90mهنوز در حال برقراری اتصال امن…\x1b[0m\r\n')
    }, 1400)
    const input = terminal.onData((data) => { if (socket.readyState === WebSocket.OPEN) socket.send(data) })
    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type !== 'keydown' || !event.ctrlKey || !event.shiftKey) return true
      if (event.key.toLowerCase() === 'c') { void copySelection(); return false }
      if (event.key.toLowerCase() === 'v') { void pasteClipboard(); return false }
      return true
    })
    const resize = () => {
      if (!host.getClientRects().length) return
      try { fit.fit(); if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'resize', cols: terminal.cols, rows: terminal.rows })) } catch { /* Ignore transient layout changes. */ }
    }
    const observer = new ResizeObserver(resize)
    observer.observe(host)
    window.addEventListener('resize', resize)
    return () => {
      disposed = true
      if (openTimer) window.clearTimeout(openTimer)
      observer.disconnect(); window.removeEventListener('resize', resize); input.dispose(); socket.close(); terminal.dispose()
      socketRef.current = null; terminalRef.current = null; fitRef.current = null
    }
  }, [session, tab.id, copySelection, pasteClipboard])

  useEffect(() => {
    if (!active) return
    requestAnimationFrame(() => {
      try {
        const terminal = terminalRef.current; const fit = fitRef.current; const socket = socketRef.current
        if (terminal && fit && hostRef.current?.getClientRects().length) {
          fit.fit()
          if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'resize', cols: terminal.cols, rows: terminal.rows }))
          terminal.focus()
        }
      } catch { /* Wait for layout to settle. */ }
    })
  }, [active])

  useEffect(() => {
    if (!active) { setMaximized(false); return }
    if (!maximized) return
    const oldOverflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape') setMaximized(false) }
    window.addEventListener('keydown', onKey)
    return () => { document.body.style.overflow = oldOverflow; window.removeEventListener('keydown', onKey) }
  }, [active, maximized])

  useEffect(() => {
    if (!active || minimized) return
    requestAnimationFrame(() => {
      try {
        const terminal = terminalRef.current; const fit = fitRef.current; const socket = socketRef.current
        if (terminal && fit && hostRef.current?.getClientRects().length) {
          fit.fit()
          if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'resize', cols: terminal.cols, rows: terminal.rows }))
        }
      } catch { /* Wait for the terminal host to finish resizing. */ }
    })
  }, [active, minimized, maximized])

  const stateLabel = connection === 'connected' ? 'متصل' : connection === 'connecting' ? 'در حال اتصال' : connection === 'error' ? 'نیاز به اتصال مجدد' : 'قطع‌شده'
  const action = (label: string, shortcut: string, icon: React.ReactNode, onClick: () => void, disabled = false) => <Tooltip key={label}><TooltipTrigger asChild><Button size="sm" variant="ghost" disabled={disabled} aria-label={label} onClick={onClick} className="h-8 gap-1.5 px-2 text-xs">{icon}<span className="hidden md:inline">{label}</span></Button></TooltipTrigger><TooltipContent>{label} <span dir="ltr" className="mr-1 font-mono opacity-70">{shortcut}</span></TooltipContent></Tooltip>

  return <div className={active ? 'block' : 'hidden'} role="tabpanel" aria-label={tab.name}>
    <div className={`overflow-hidden border-border bg-[#090d12] ${maximized ? 'fixed inset-2 z-[80] flex flex-col rounded-2xl border shadow-2xl sm:inset-4' : ''}`}>
      <div className={`flex flex-wrap items-center justify-between gap-2 border-b px-3 py-2 ${tab.mode === 'root' ? 'border-rose-300/20 bg-rose-950/30' : 'border-white/10 bg-[#111820]/90'}`}>
      <div className="flex min-w-0 items-center gap-2"><div className="flex items-center gap-1.5" aria-label="کنترل‌های پنجرهٔ ترمینال">
        <Tooltip><TooltipTrigger asChild><button type="button" aria-label="بستن تب ترمینال" onClick={onClose} className="grid h-3 w-3 place-items-center rounded-full bg-rose-400 transition-[filter,transform] hover:scale-110 hover:brightness-110 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-rose-300"><X size={8} className="opacity-0 hover:opacity-100"/></button></TooltipTrigger><TooltipContent>بستن تب و پایان نشست</TooltipContent></Tooltip>
        <Tooltip><TooltipTrigger asChild><button type="button" aria-label={minimized ? 'بازگردانی ترمینال' : 'کوچک‌کردن ترمینال'} onClick={() => setMinimized((value) => !value)} className="grid h-3 w-3 place-items-center rounded-full bg-amber-300 transition-[filter,transform] hover:scale-110 hover:brightness-110 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-amber-200"><Minimize2 size={8} className="opacity-0 hover:opacity-100"/></button></TooltipTrigger><TooltipContent>{minimized ? 'بازگرداندن پنجره' : 'کوچک‌کردن پنجره؛ نشست باز می‌ماند'}</TooltipContent></Tooltip>
        <Tooltip><TooltipTrigger asChild><button type="button" aria-label={maximized ? 'بازگردانی اندازهٔ ترمینال' : 'بزرگ‌کردن ترمینال'} onClick={() => { setMinimized(false); setMaximized((value) => !value) }} className="grid h-3 w-3 place-items-center rounded-full bg-emerald-400 transition-[filter,transform] hover:scale-110 hover:brightness-110 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-emerald-300">{maximized && <Minimize2 size={8} className="opacity-0 hover:opacity-100"/>}</button></TooltipTrigger><TooltipContent>{maximized ? 'بازگردانی اندازه · Esc' : 'بزرگ‌کردن به اندازهٔ صفحه'}</TooltipContent></Tooltip>
      </div><span className={`mr-1 max-w-40 truncate font-mono text-xs ${tab.mode === 'root' ? 'text-rose-200' : 'text-muted-foreground'}`} dir="ltr">{tab.name} — {tab.mode === 'root' ? 'root@server' : 'arg@server'}</span><Badge variant={connection === 'connected' ? 'success' : connection === 'error' ? 'warning' : 'outline'}><Circle size={8} className={connection === 'connected' ? 'fill-current' : ''}/>{stateLabel}</Badge>{connection === 'connecting' && <span className="flex items-center gap-1 text-[11px] text-muted-foreground"><LoaderCircle size={12} className="animate-spin"/>اتصال امن در حال آماده‌سازی است</span>}</div>
      <div className="flex flex-wrap items-center gap-0.5">
        {action('کپی', 'Ctrl ⇧ C', <Copy size={14}/>, () => void copySelection())}
        {action('برش', 'انتخاب و کپی', <Scissors size={14}/>, () => void copySelection(true))}
        {action('پیست', 'Ctrl ⇧ V', <ClipboardPaste size={14}/>, () => void pasteClipboard())}
        {action('پاک‌کردن صفحه', 'Ctrl L', <Trash2 size={14}/>, () => terminalRef.current?.clear(), connection !== 'connected')}
        {action('اتصال دوباره', 'Reopen PTY', <RotateCw size={14}/>, () => { setError(''); setConnection('connecting'); setSession((value) => value + 1) })}
      </div>
    </div>
    {!minimized && connection === 'error' && <div className="mx-3 mt-3 rounded-xl border border-rose-400/20 bg-rose-400/[.06] px-3 py-2 text-xs text-rose-100" role="status">{error} <button className="mr-2 underline underline-offset-2" onClick={() => { setError(''); setConnection('connecting'); setSession((value) => value + 1) }}>تلاش دوباره</button></div>}
    {!minimized && clipboardNote && active && <div className="mx-3 mt-2 flex items-center gap-2 text-[11px] text-[#dda46f]" role="status"><Clipboard size={12}/>{clipboardNote}</div>}
    <div ref={hostRef} className={`w-full p-3 sm:p-4 ${minimized ? 'hidden' : maximized ? 'min-h-0 flex-1' : 'h-[min(66vh,720px)] min-h-[380px]'}`} dir="ltr" aria-label={`ترمینال تعاملی ${tab.name}`} />
    {!minimized ? <div className="flex flex-wrap items-center justify-between gap-2 border-t border-white/5 px-4 py-2 text-[10px] text-muted-foreground"><span>انتخاب متن با ماوس · اسکرول برای تاریخچه · Ctrl+C برای ارسال وقفه به شل</span><span dir="ltr" className="font-mono">Ctrl + Shift + C copy · Ctrl + Shift + V paste</span></div> : <div className="flex items-center justify-between px-4 py-2 text-xs text-muted-foreground"><span>ترمینال کوچک شده است؛ نشست سرور همچنان فعال است.</span><Button size="sm" variant="ghost" onClick={() => setMinimized(false)}><Maximize2 size={14}/>بازگردانی</Button></div>}
    </div>
  </div>
}
