import { createContext, useContext } from 'react'

export const PanelTimeZoneContext = createContext('UTC')

export function usePanelTimeZone() {
  return useContext(PanelTimeZoneContext)
}

export function formatPanelDate(
  seconds: number | null,
  timeZone: string,
  options: Intl.DateTimeFormatOptions = { dateStyle: 'medium', timeStyle: 'short' },
  locale = 'en-US',
) {
  if (seconds === null) return '—'
  return new Intl.DateTimeFormat(locale, { ...options, timeZone }).format(seconds * 1000)
}
