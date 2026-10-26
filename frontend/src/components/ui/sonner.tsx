import { Toaster as Sonner, type ToasterProps } from 'sonner'

const Toaster = ({ ...props }: ToasterProps) => (
  <Sonner
    dir="rtl"
    position="bottom-left"
    closeButton
    richColors
    toastOptions={{
      classNames: {
        toast: 'arg-glass-toast !rounded-2xl',
        success: '!border-emerald-400/35 !bg-emerald-950/75 !text-emerald-100',
        error: '!border-rose-400/40 !bg-rose-950/80 !text-rose-100',
        warning: '!border-amber-400/40 !bg-amber-950/80 !text-amber-100',
        info: '!border-sky-400/35 !bg-sky-950/75 !text-sky-100',
        title: '!text-sm !font-semibold',
        description: '!text-xs !leading-6 !text-muted-foreground',
        actionButton: '!bg-primary !text-primary-foreground',
        cancelButton: '!bg-secondary !text-secondary-foreground',
        closeButton: '!border-border !bg-secondary !text-muted-foreground',
      },
    }}
    {...props}
  />
)

export { Toaster }
