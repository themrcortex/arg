import { Moon, Sun } from 'lucide-react'
import { Button } from '../ui/button'
import { useTheme } from '../../lib/theme'

export function ThemeToggle() {
  const { theme, toggleTheme } = useTheme()
  const nextTheme = theme === 'dark' ? 'light' : 'dark'
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      className="h-9 w-9 shrink-0 text-muted-foreground hover:text-primary"
      onClick={toggleTheme}
      aria-label={`تغییر به تم ${nextTheme === 'light' ? 'روشن' : 'تیره'}`}
      title={`تم ${nextTheme === 'light' ? 'روشن' : 'تیره'}`}
    >
      {theme === 'dark' ? <Sun size={17} /> : <Moon size={17} />}
    </Button>
  )
}
