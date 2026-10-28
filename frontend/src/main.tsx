import React from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import './index.css'
import { ThemeProvider } from './lib/theme'

if (import.meta.env.PROD && 'serviceWorker' in navigator) {
  const hadController = Boolean(navigator.serviceWorker.controller)
  window.addEventListener('load', () => {
    let registration: ServiceWorkerRegistration | undefined
    const checkForUpdate = () => {
      if (document.visibilityState === 'visible') void registration?.update().catch(() => {})
    }
    navigator.serviceWorker.addEventListener('controllerchange', () => {
      if (!hadController) return
      window.location.reload()
    })
    void navigator.serviceWorker.register('/service-worker.js', { scope: '/' }).then((value) => {
      registration = value
      checkForUpdate()
      window.setInterval(checkForUpdate, 5 * 60 * 1000)
      window.addEventListener('focus', checkForUpdate)
      document.addEventListener('visibilitychange', checkForUpdate)
    }).catch(() => {
      // Offline shell support is optional; the online panel remains usable if registration is blocked.
    })
  }, { once: true })
}

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <ThemeProvider>
      <App />
    </ThemeProvider>
  </React.StrictMode>,
)
