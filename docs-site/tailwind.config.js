/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ['class'],
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: { extend: { colors: {
    background: 'hsl(var(--background))', foreground: 'hsl(var(--foreground))', card: 'hsl(var(--card))',
    primary: 'hsl(var(--primary))', muted: 'hsl(var(--muted))', 'muted-foreground': 'hsl(var(--muted-foreground))',
    border: 'hsl(var(--border))', accent: 'hsl(var(--accent))'
  }, borderRadius: { xl: 'var(--radius)' } } },
  plugins: [],
}
