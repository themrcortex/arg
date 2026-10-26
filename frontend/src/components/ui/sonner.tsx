import { Toaster as Sonner, type ToasterProps } from 'sonner'

const Toaster = ({ ...props }: ToasterProps) => (
  <Sonner
    dir="rtl"
    position="bottom-left"
    closeButton
    richColors
    toastOptions={{
      classNames: {
        toast: 'arg-glass-popover !rounded-2xl !text-card-foreground',
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
