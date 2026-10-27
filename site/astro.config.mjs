import { defineConfig } from "astro/config";
import { fileURLToPath } from "node:url";
import remarkDocs from "./scripts/remark-docs.mjs";

const base = process.env.ASTRO_BASE ?? "/";
const prefix = base.replace(/\/$/, "");

export default defineConfig({
  site: process.env.ASTRO_SITE ?? "https://dodecahedr0x.github.io",
  base,
  trailingSlash: "never",
  redirects: {
    "/docs/getting-started": `${prefix}/guides/getting-started`,
    "/compare": `${prefix}/guides/comparison`,
    "/interop": `${prefix}/docs/interop`,
  },
  markdown: {
    remarkPlugins: [[remarkDocs, { base }]],
    shikiConfig: {
      theme: "github-dark-dimmed",
    },
  },
  vite: {
    server: {
      fs: {
        allow: [fileURLToPath(new URL("..", import.meta.url))],
      },
    },
  },
});
