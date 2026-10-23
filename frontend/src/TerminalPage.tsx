import { useEffect, useRef, useState } from 'react'
import { FitAddon } from '@xterm/addon-fit'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { Check, Circle, RotateCw, Trash2 } from 'lucide-react'
import { Alert, AlertDescription } from './components/ui/alert'
import { Button } from './components/ui/button'
import { Badge } from './components/ui/badge'

type Connection = 'connecting' | 'connected' | 'disconnected' | 'error'

export function TerminalPage({ Heading }: { Heading: (props: { title: string; description: string }) => React.ReactNode }) {
  const hostRef = useRef<HTMLDivElement>(null)
  const terminalRef = useRef<Terminal | null>(null)
  const [connection, setConnection] = useState<Connection>('connecting')
  const [session, setSession] = useState(0)
  const [error, setError] = useState('')

  useEffect(() => {
    const host = hostRef.current
    if (!host) return
    const terminal = new Terminal({
      cursorBlink: true,
      convertEol: true,
      scrollback: 5000,
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
      fontSize: 13,
      lineHeight: 1.25,
      theme: {
        background: '#090d12', foreground: '#e8edf2', cursor: '#dda46f', cursorAccent: '#090d12',
        selectionBackground: '#dda46f55', black: '#111820', red: '#e87575', green: '#53c992', yellow: '#dda46f',
        blue: '#72aee6', magenta: '#b892e8', cyan: '#58c4c0', white: '#e8edf2', brightBlack: '#63717d',
        brightRed: '#ff9292', brightGreen: '#70e0a9', brightYellow: '#f0bf85', brightBlue: '#8bc1f5',
        brightMagenta: '#c7a8f0', brightCyan: '#71d8d3', brightWhite: '#ffffff',
      },
      allowProposedApi: false,
    })
    const fit = new FitAddon()
    terminal.loadAddon(fit)
    terminal.open(host)
    terminalRef.current = terminal
    terminal.writeln('\x1b[38;2;221;164;111mARG SERVER TERMINAL\x1b[0m')
    terminal.writeln('Starting a secure PTY session…\r\n')

    let socket: WebSocket
    try {
      const scheme = location.protocol === 'https:' ? 'wss:' : 'ws:'
      socket = new WebSocket(`${scheme}//${location.host}/api/terminal`)
      socket.binaryType = 'arraybuffer'
    } catch {
      setConnection('error')
      setError('مرورگر نتوانست اتصال WebSocket را بسازد.')
      return () => terminal.dispose()
    }
    socket.onopen = () => {
      setConnection('connected')
      setError('')
      requestAnimationFrame(() => {
        try { fit.fit(); socket.send(JSON.stringify({ type: 'resize', cols: terminal.cols, rows: terminal.rows })) } catch { /* Terminal may be hidden during initial layout. */ }
        terminal.focus()
      })
    }
    socket.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) terminal.write(new Uint8Array(event.data))
      else if (typeof event.data === 'string') terminal.write(event.data)
    }
    socket.onerror = () => {
      setConnection('error')
      setError('اتصال ترمینال برقرار نشد. دسترسی حساب مدیر و سرویس پنل را بررسی کن.')
    }
    socket.onclose = () => setConnection((current) => current === 'error' ? current : 'disconnected')
    const input = terminal.onData((data) => { if (socket.readyState === WebSocket.OPEN) socket.send(data) })
    const resize = () => {
      try {
        fit.fit()
        if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'resize', cols: terminal.cols, rows: terminal.rows }))
      } catch { /* Ignore resize while the terminal is hidden. */ }
    }
    const observer = new ResizeObserver(resize)
    observer.observe(host)
    window.addEventListener('resize', resize)
    return () => {
      observer.disconnect()
      window.removeEventListener('resize', resize)
      input.dispose()
      socket.close()
      terminal.dispose()
      terminalRef.current = null
    }
  }, [session])

  const stateLabel = connection === 'connected' ? 'متصل' : connection === 'connecting' ? 'در حال اتصال' : connection === 'error' ? 'خطا در اتصال' : 'قطع‌شده'
  return <>
    <Heading title="ترمینال سرور" description="یک نشست تعاملی واقعی روی PTY سرور؛ ورودی، خروجی و تغییر اندازهٔ ترمینال زنده منتقل می‌شود." />
    <Alert className="mb-4 border-amber-400/20 bg-amber-400/[.05]"><AlertDescription>این نشست با دسترسی محدود کاربر سرویس ARG اجرا می‌شود و دستورات مدیریتی نیازمند sudo در دسترس نیستند. با بستن صفحه، فرایند ترمینال خاتمه می‌یابد.</AlertDescription></Alert>
    {error && <Alert className="mb-4 border-rose-400/20 bg-rose-400/[.05]"><AlertDescription>{error}</AlertDescription></Alert>}
    <section className="overflow-hidden rounded-2xl border border-border bg-[#090d12] shadow-2xl shadow-black/20">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-white/10 bg-[#111820] px-4 py-3">
        <div className="flex items-center gap-2"><span className="flex gap-1.5" aria-hidden="true"><i className="h-2.5 w-2.5 rounded-full bg-rose-400"/><i className="h-2.5 w-2.5 rounded-full bg-amber-300"/><i className="h-2.5 w-2.5 rounded-full bg-emerald-400"/></span><span className="mr-2 font-mono text-xs text-muted-foreground" dir="ltr">bash — arg@server</span><Badge variant={connection === 'connected' ? 'success' : connection === 'error' ? 'warning' : 'outline'}><Circle size={8} className={connection === 'connected' ? 'fill-current' : ''}/>{stateLabel}</Badge></div>
        <div className="flex gap-1"><Button size="sm" variant="ghost" disabled={connection !== 'connected'} onClick={() => terminalRef.current?.clear()}><Trash2 size={14}/>پاک‌کردن صفحه</Button><Button size="sm" variant="outline" onClick={() => { setError(''); setConnection('connecting'); setSession((value) => value + 1) }}><RotateCw size={14}/>اتصال دوباره</Button></div>
      </div>
      <div ref={hostRef} className="h-[min(66vh,720px)] min-h-[380px] w-full p-3 sm:p-4" dir="ltr" aria-label="ترمینال تعاملی سرور" />
    </section>
    <div className="mt-3 flex items-center gap-2 text-[11px] text-muted-foreground"><Check size={13} className="text-emerald-300"/> نشست رمزنگاری‌شده از همان اتصال پنل و محافظت‌شده با نشست ورود مدیر</div>
  </>
}
