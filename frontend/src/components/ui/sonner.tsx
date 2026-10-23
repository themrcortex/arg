import { Toaster as Sonner, type ToasterProps } from 'sonner'

const Toaster = ({ ...props }: ToasterProps) => (
  <Sonner
    dir="rtl"
    position="bottom-left"
    closeButton
    richColors
    toastOptions={{
      classNames: {
        toast: '!rounded-2xl !border-border !bg-card !text-card-foreground !shadow-2xl',
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
