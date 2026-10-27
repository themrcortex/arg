import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig({
  plugins: [react()],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  base: '/',
  server: {
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8282',
        changeOrigin: true,
        configure(proxy) {
          proxy.on('proxyReq', (request) => request.setHeader('Origin', 'http://127.0.0.1:8282'))
        },
      },
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    cssCodeSplit: true,
    rollupOptions: {
      output: {
        entryFileNames: 'assets/[name]-[hash].js',
        chunkFileNames: 'assets/[name]-[hash].js',
        assetFileNames: 'assets/[name]-[hash][extname]',
      },
    },
  },
})
