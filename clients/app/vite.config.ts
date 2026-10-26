import { fileURLToPath, URL } from "node:url";

import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

/** Where `pnpm dev` sends the server's own paths (the API, the BFF). */
const server = process.env["APPLICAMP_API_URL"] ?? "http://localhost:8080";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: [{ find: "~", replacement: fileURLToPath(new URL("src", import.meta.url)) }],
  },
  server: {
    port: 5173,
    strictPort: true,
    // The API and the BFF on the dev server's origin, so the session cookie
    // and the same-origin checks work as in production. Sign-in comes back to
    // APPLICAMP_PUBLIC_URL: run the server with
    // APPLICAMP_PUBLIC_URL=http://localhost:5173 while you use `pnpm dev`.
    proxy: {
      "/api": { target: server },
      "/bff": { target: server },
    },
  },
  build: {
    // The server serves dist/ (APPLICAMP_WEB_DIR); everything in assets/ is
    // named after its content and cached for good.
    assetsDir: "assets",
    sourcemap: false,
  },
});
