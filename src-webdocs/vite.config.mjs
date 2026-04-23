import { defineConfig } from "vite";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";

const rootDir = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
  base: "./",
  plugins: [tailwindcss()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      input: {
        index: resolve(rootDir, "index.html"),
        privacyPolicy: resolve(rootDir, "privacy-policy.html"),
        termsOfUse: resolve(rootDir, "terms-of-use.html"),
      },
    },
  },
  server: {
    host: "127.0.0.1",
    port: 4174,
    strictPort: true,
  },
  clearScreen: false,
});
