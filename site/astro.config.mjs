import { defineConfig } from "astro/config";
import { fileURLToPath } from "node:url";

const base = process.env.ASTRO_BASE ?? "/";
const root = base.replace(/\/$/, "");

function prefix(path) {
  return `${root}${path}`;
}

function rewrite(url) {
  if (!url || typeof url !== "string") return url;
  if (url === "./http/openapi.yaml" || url.endsWith("/http/openapi.yaml")) {
    return prefix("/openapi.yaml");
  }
  const m = url.match(/^\.\/\d+-([a-z0-9-]+)\.md(.*)$/);
  if (!m) return url;
  const slug = m[1];
  const hash = m[2] || "";
  if (slug === "terminology") return prefix(`/terminology${hash}`);
  return prefix(`/spec/${slug}${hash}`);
}

function walk(node) {
  if (!node || typeof node !== "object") return;
  if (node.type === "link" || node.type === "definition") node.url = rewrite(node.url);
  const kids = node.children;
  if (Array.isArray(kids)) kids.forEach(walk);
}

function remarkSpecLinks() {
  return (tree) => walk(tree);
}

export default defineConfig({
  site: process.env.ASTRO_SITE ?? "https://dodecahedr0x.github.io",
  base,
  trailingSlash: "never",
  markdown: {
    remarkPlugins: [remarkSpecLinks],
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
