import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

export default defineConfig({
  root: fileURLToPath(new URL(".", import.meta.url)),
  publicDir: fileURLToPath(new URL("../../public", import.meta.url)),
  plugins: [react()],
  // Shared property components import these origins. All preview requests stay
  // relative; this is a separate build, never part of the production entry point.
  define: {
    "import.meta.env.VITE_API_BASE": JSON.stringify("https://inventory-preview.invalid"),
    "import.meta.env.VITE_SITE_URL": JSON.stringify("https://inventory-preview.invalid"),
  },
  build: { outDir: "dist", emptyOutDir: true, copyPublicDir: false },
  server: {
    host: "127.0.0.1", port: 5191, strictPort: true,
    fs: { allow: [fileURLToPath(new URL("../../../", import.meta.url))] },
    proxy: { "/api/inventory": "http://127.0.0.1:4019", "/media": "http://127.0.0.1:4000" },
  },
});
