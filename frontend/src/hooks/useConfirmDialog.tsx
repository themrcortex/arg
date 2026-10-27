import { useCallback, useRef, useState } from 'react'
import { AlertTriangle, ShieldAlert } from 'lucide-react'
import { Button } from '../components/ui/button'
import { Input } from '../components/ui/input'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '../components/ui/dialog'

export type ConfirmOptions = {
  title?: string
  description: string
  confirmLabel?: string
  cancelLabel?: string
  /** Require the user to type this exact value before continuing. */
  phrase?: string
  destructive?: boolean
}

export function useConfirmDialog() {
  const [request, setRequest] = useState<ConfirmOptions | null>(null)
  const [typed, setTyped] = useState('')
  const resolver = useRef<((confirmed: boolean) => void) | null>(null)

  const finish = useCallback((confirmed: boolean) => {
    resolver.current?.(confirmed)
    resolver.current = null
    setRequest(null)
    setTyped('')
  }, [])

  const confirm = useCallback((options: ConfirmOptions) => new Promise<boolean>((resolve) => {
    if (resolver.current) resolver.current(false)
    resolver.current = resolve
    setTyped('')
    setRequest(options)
  }), [])

  const dialog = (
    <Dialog open={request !== null} onOpenChange={(open) => { if (!open && resolver.current) finish(false) }}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {request?.destructive ? <ShieldAlert size={19} className="text-destructive"/> : <AlertTriangle size={19} className="text-amber-500"/>}
            {request?.title || 'تأیید عملیات'}
          </DialogTitle>
          <DialogDescription className="leading-6">{request?.description}</DialogDescription>
        </DialogHeader>
        {request?.phrase !== undefined && <label className="mb-4 block space-y-2 text-xs text-muted-foreground">
          <span>برای ادامه، این عبارت را دقیق وارد کن:</span>
          <code dir="ltr" className="block break-all rounded-lg border border-border bg-background/70 p-2 font-mono text-foreground">{request.phrase}</code>
          <Input autoComplete="off" spellCheck={false} dir="ltr" value={typed} onChange={(event) => setTyped(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter' && typed === request.phrase) finish(true) }} aria-label="عبارت تأیید"/>
        </label>}
        <div className="flex flex-wrap justify-end gap-2">
          <Button type="button" variant="outline" onClick={() => finish(false)}>{request?.cancelLabel || 'انصراف'}</Button>
          <Button type="button" variant={request?.destructive ? 'destructive' : 'default'} disabled={request?.phrase !== undefined && typed !== request.phrase} onClick={() => finish(true)}>{request?.confirmLabel || 'تأیید و ادامه'}</Button>
        </div>
      </DialogContent>
    </Dialog>
  )

  return { confirm, confirmDialog: dialog }
}
