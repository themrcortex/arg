import type { ReactNode } from 'react'

type AppShellProps = { sidebar: ReactNode; children: ReactNode; collapsed: boolean }

export function AppShell({ sidebar, children, collapsed }: AppShellProps) {
  return (
    <div
      className="app-shell min-h-dvh lg:grid"
      style={{ gridTemplateColumns: `${collapsed ? 56 : 224}px minmax(0, 1fr)` }}
    >
      {sidebar}
      {children}
    </div>
  )
}
