export type Role = 'admin' | 'operator' | 'viewer'
export type User = { id: number; username: string; role: Role; enabled: boolean }
export type Interface = {
  name: string
  state: string
  mac: string
  rx_bytes: number
  tx_bytes: number
  rx_packets?: number
  tx_packets?: number
  rx_errors?: number
  tx_errors?: number
  rx_dropped?: number
  tx_dropped?: number
  mtu?: number | null
  speed_mbps?: number | null
  carrier?: boolean
  addresses?: string[]
}
export type Port = { port: number; protocol: string; ipv6: boolean }
export type StorageVolume = {
  id: string
  device: string
  device_id: string
  mount_point: string
  filesystem: string
  kind: 'local' | 'cloud' | 'remote' | 'virtual'
  total_bytes: number | null
  used_bytes: number | null
  available_bytes: number | null
  reserved_bytes: number | null
  use_percent: number | null
  read_bytes_per_sec?: number | null
  write_bytes_per_sec?: number | null
  read_only: boolean
  mount_status: 'mounted' | 'unknown'
  is_remote: boolean
  is_virtual: boolean
  is_cloud: boolean
  measured_at: number | null
  error: string | null
}
export type StorageInventory = {
  volumes: StorageVolume[]
  local: { total_bytes: number; used_bytes: number; available_bytes: number; reserved_bytes: number; aggregation_reliable: boolean; aggregation_basis: string }
  measured_at: number
}
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
  cpu_cores?: number
  memory_details?: {
    free_kib?: number
    buffers_kib?: number
    cached_kib?: number
    active_kib?: number
    inactive_kib?: number
    dirty_kib?: number
    slab_kib?: number
  }
  process_rss_bytes: number
  process_cpu_percent: number
  disk_total_bytes?: number
  disk_used_bytes?: number
  disk_free_bytes?: number
  storage?: StorageInventory
  interfaces: Interface[]
  ports: Port[]
}
export type AuditEvent = { at: number; actor: string; action: string; detail: string }
export type Firewall = { available: boolean; status: string; error?: string }
export type ManagedFirewallRule = { id: number; port: number; protocol: 'tcp' | 'udp'; policy: 'allow' | 'deny'; enabled: boolean }
export type ManagedFirewallRulesResponse = { rules: ManagedFirewallRule[]; protected_ports?: number[] }

export type ApiDiagnostic = Record<string, unknown> & {
  code?: string
  source?: string
  operation?: string
  field?: string
  raw?: string
  expected?: string
  hint?: string
}
export type ApiFailureReport = { message: string; path: string; status: number; at: string; diagnostic?: ApiDiagnostic }

export class ApiError extends Error {
  readonly status: number
  readonly path: string
  readonly diagnostic?: ApiDiagnostic
  constructor(report: ApiFailureReport) {
    super(report.message)
    this.name = 'ApiError'
    this.status = report.status
    this.path = report.path
    this.diagnostic = report.diagnostic
  }
}

let latestApiFailure: ApiFailureReport | null = null
export function getLastApiFailure() { return latestApiFailure }
function recordApiFailure(report: ApiFailureReport) { latestApiFailure = report }

const ACTIVE_NODE_KEY = 'arg.active-workspace-node'
export function getActiveWorkspaceNode() {
  try {
    const current = window.sessionStorage.getItem(ACTIVE_NODE_KEY)
    if (current) return current
    const previous = window.localStorage.getItem(ACTIVE_NODE_KEY)
    if (!previous) return ''
    window.sessionStorage.setItem(ACTIVE_NODE_KEY, previous)
    window.localStorage.removeItem(ACTIVE_NODE_KEY)
    return previous
  } catch { return '' }
}
export function setActiveWorkspaceNode(id: string) {
  try { if (id) window.sessionStorage.setItem(ACTIVE_NODE_KEY, id); else window.sessionStorage.removeItem(ACTIVE_NODE_KEY) } catch { /* Workspace selection can be unavailable under strict storage policies. */ }
}
function isLocalOnlyApiPath(path: string) {
  const route = path.split('?', 1)[0].replace(/^\/+/, '')
  // The global updater owns this ARG process; remote Nodes use the dedicated nodes/{id}/update API.
  return /^(state|setup|login|logout|account(?:\/|$)|nodes(?:\/|$)|federation(?:\/|$)|update(?:\/|$)|admin\/api-keys(?:\/|$))/.test(route)
}
export function apiUrl(path: string) {
  const node = getActiveWorkspaceNode()
  const route = path.replace(/^\/+/, '')
  if (!node || node === 'all' || isLocalOnlyApiPath(route)) return `/api/${route}`
  const slash = route.indexOf('?')
  const endpoint = slash < 0 ? route : route.slice(0, slash)
  const query = slash < 0 ? '' : route.slice(slash)
  return `/api/nodes/${encodeURIComponent(node)}/proxy/${endpoint}${query}`
}
export function terminalSocketUrl(path: string) {
  const node = getActiveWorkspaceNode()
  const suffix = path.includes('?') ? path.slice(path.indexOf('?')) : ''
  if (node) return `${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/api/nodes/${encodeURIComponent(node)}/workspace-terminal${suffix}`
  return `${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/api/${path.replace(/^\/+/, '')}`
}

async function requestApi<T>(url: string, path: string, init?: RequestInit): Promise<T> {
  let response: Response
  try {
    response = await fetch(url, {
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json' },
      ...init,
    })
  } catch (cause) {
    const message = cause instanceof Error ? cause.message : 'Network request failed'
    const report = { message, path, status: 0, at: new Date().toISOString() }
    recordApiFailure(report)
    throw new ApiError(report)
  }
  const result: unknown = await response.json().catch(() => null)
  if (!response.ok) {
    const payload = result && typeof result === 'object' ? result as Record<string, unknown> : {}
    const diagnostic = payload.diagnostic && typeof payload.diagnostic === 'object'
      ? payload.diagnostic as ApiDiagnostic
      : undefined
    const report: ApiFailureReport = {
      message: typeof payload.error === 'string' ? payload.error : `HTTP ${response.status}`,
      path,
      status: response.status,
      at: new Date().toISOString(),
      diagnostic,
    }
    recordApiFailure(report)
    throw new ApiError(report)
  }
  return result as T
}

export function api<T>(path: string, init?: RequestInit): Promise<T> {
  return requestApi<T>(apiUrl(path), path, init)
}

/** Requests owned by this control plane (fleet metadata, local registry, etc.). */
export function apiControlPlane<T>(path: string, init?: RequestInit): Promise<T> {
  const route = path.replace(/^\/+/, '')
  return requestApi<T>(`/api/${route}`, path, init)
}

/** Fetch one server's API through the authenticated controller proxy. */
export function apiForWorkspace<T>(workspaceKey: string, path: string, init?: RequestInit): Promise<T> {
  const route = path.replace(/^\/+/, '')
  if (!workspaceKey || workspaceKey === 'all' || isLocalOnlyApiPath(route)) {
    return Promise.reject(new ApiError({ message: 'این API برای یک سرور مشخص لازم است.', path, status: 400, at: new Date().toISOString() }))
  }
  const slash = route.indexOf('?')
  const endpoint = slash < 0 ? route : route.slice(0, slash)
  const query = slash < 0 ? '' : route.slice(slash)
  const url = `/api/nodes/${encodeURIComponent(workspaceKey)}/proxy/${endpoint}${query}`
  return requestApi<T>(url, path, init)
}
export function formatNumber(n: number) {
  return new Intl.NumberFormat('en-US').format(n || 0)
}
export function formatBytes(n: number) {
  let value = Number(n) || 0
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  let index = 0
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024
    index++
  }
  return `${new Intl.NumberFormat('en-US', { maximumFractionDigits: 1 }).format(value)} ${units[index]}`
}
export function formatUptime(seconds: number) {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  if (days) return `${formatNumber(days)} روز و ${formatNumber(hours)} ساعت`
  if (hours) return `${formatNumber(hours)} ساعت و ${formatNumber(minutes)} دقیقه`
  return `${formatNumber(minutes)} دقیقه`
}
