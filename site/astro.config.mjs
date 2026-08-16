import { defineConfig } from "astro/config";
import { fileURLToPath } from "node:url";

function rewrite(url) {
  if (!url || typeof url !== "string") return url;
  if (url === "./http/openapi.yaml" || url.endsWith("/http/openapi.yaml")) {
    return "/openapi.yaml";
  }
  const m = url.match(/^\.\/\d+-([a-z0-9-]+)\.md(.*)$/);
  if (!m) return url;
  const slug = m[1];
  const hash = m[2] || "";
  if (slug === "terminology") return `/terminology${hash}`;
  return `/spec/${slug}${hash}`;
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
  site: "https://tangled.protocol",
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
