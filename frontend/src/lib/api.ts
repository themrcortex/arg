export type Role = 'admin' | 'operator' | 'viewer'
export type User = { id: number; username: string; role: Role; enabled: boolean }
export type Interface = {
  name: string
  state: string
  mac: string
  rx_bytes: number
  tx_bytes: number
  addresses?: string[]
}
export type Port = { port: number; protocol: string; ipv6: boolean }
export type Dashboard = {
  hostname: string
  uptime_seconds: number
  load: string
  memory_kib: number
  available_kib: number
  swap_total_kib?: number
  swap_free_kib?: number
  cpu_total?: number
  cpu_idle?: number
  process_rss_bytes: number
  process_cpu_percent: number
  disk_total_bytes?: number
  disk_free_bytes?: number
  interfaces: Interface[]
  ports: Port[]
}
export type AuditEvent = { at: number; actor: string; action: string; detail: string }
export type Firewall = { available: boolean; status: string; error?: string }

export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api/${path}`, {
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' },
    ...init,
  })
  const result = await response.json()
  if (!response.ok) throw new Error(result.error || `HTTP ${response.status}`)
  return result as T
}
export function formatNumber(n: number) {
  return new Intl.NumberFormat('fa-IR').format(n || 0)
}
export function formatBytes(n: number) {
  let value = Number(n) || 0
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let index = 0
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024
    index++
  }
  return `${new Intl.NumberFormat('fa-IR', { maximumFractionDigits: 1 }).format(value)} ${units[index]}`
}
export function formatUptime(seconds: number) {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  return `${formatNumber(days)} روز و ${formatNumber(hours)} ساعت`
}
