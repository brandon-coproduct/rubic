import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Backend runs on :3000; the SPA dev server runs on :5173.
// We proxy /api and /healthz so fetches from the browser land at the
// Rust server without CORS round-trips.
export default defineConfig({
  plugins: [svelte()],
  server: {
    port: 5173,
    proxy: {
      '/api': { target: 'http://127.0.0.1:3000', changeOrigin: true },
      '/healthz': { target: 'http://127.0.0.1:3000', changeOrigin: true },
    },
  },
});
