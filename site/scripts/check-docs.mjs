import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { relative, resolve } from "node:path";
import { docsRoot, specFiles } from "./docs.mjs";

const dist = resolve("dist");
const base = (process.env.ASTRO_BASE || "/").replace(/\/$/, "");
const walk = (dir) => readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? walk(path) : [path];
});
const html = walk(dist).filter((path) => path.endsWith(".html"));
const failures = [];
let links = 0;
for (const file of html) {
  const body = readFileSync(file, "utf8");
  for (const [, href] of body.matchAll(/href="([^"]+)"/g)) {
    if (/^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(href)) continue;
    const pagePath = `/${relative(dist, file).replace(/index\.html$/, "")}`;
    const url = new URL(href.replace(/&amp;/g, "&"), `https://docs.test${base}${pagePath}`);
    if (base && !url.pathname.startsWith(`${base}/`) && url.pathname !== base) {
      failures.push(`${relative(dist, file)}: missing base in ${href}`);
      continue;
    }
    const path = decodeURIComponent(url.pathname.slice(base.length)).replace(/^\//, "");
    let target = resolve(dist, path);
    if (existsSync(target) && statSync(target).isDirectory()) target = resolve(target, "index.html");
    if (!existsSync(target) && !path.includes(".")) target = resolve(dist, path, "index.html");
    if (!existsSync(target)) {
      failures.push(`${relative(dist, file)}: missing target ${href}`);
      continue;
    }
    if (url.hash && target.endsWith(".html")) {
      const fragment = decodeURIComponent(url.hash.slice(1));
      const targetBody = readFileSync(target, "utf8");
      if (!targetBody.includes(`id="${fragment}"`)) failures.push(`${relative(dist, file)}: missing fragment ${href}`);
    }
    links++;
  }
}
for (const file of specFiles()) {
  const published = resolve(dist, "spec", relative(resolve(docsRoot, "spec"), file));
  assert.deepEqual(readFileSync(published), readFileSync(file), `Download differs: ${file}`);
}
assert.deepEqual(readFileSync(resolve(dist, "openapi.yaml")), readFileSync(resolve(docsRoot, "spec/http/openapi.yaml")));
assert.ok(existsSync(resolve(dist, "pagefind/pagefind.js")), "Search index missing");
assert.ok(!existsSync(resolve(dist, "docs/internal")) && !existsSync(resolve(dist, "docs/superpowers")), "Internal docs published");
const pagination = new Map();
const levelBoundaries = new Map();
for (const file of html) {
  const route = relative(dist, file);
  const page = readFileSync(file, "utf8");
  const sidebar = page.match(/<nav class="sidebar"[^>]*>([\s\S]*?)<\/nav>/)?.[1];
  if (!sidebar) continue; // Landing and legacy redirects do not have sidebars.
  const selected = route.startsWith("guides/") ? "Guides" : route.startsWith("docs/") ? "Docs" : "Specs";
  for (const level of ["Guides", "Docs", "Specs"]) {
    assert.equal(sidebar.includes(`aria-label="${level}"`), level === selected,
      `${route}: sidebar must show only ${selected}, found ${level}`);
  }
  const allowed = selected === "Guides" ? ["/guides"] : selected === "Docs" ? ["/docs"]
    : ["/spec", "/reference", "/terminology", "/playground"];
  const sidebarPaths = [];
  for (const [, href] of sidebar.matchAll(/href="([^"]+)"/g)) {
    const path = href.slice(base.length);
    sidebarPaths.push(path);
    assert.ok(allowed.some((prefix) => path === prefix || path.startsWith(`${prefix}/`)),
      `${route}: sidebar links outside ${selected}: ${href}`);
  }
  levelBoundaries.set(selected, [sidebarPaths[0], sidebarPaths.at(-1)]);
  const topNav = page.match(/<nav class="site-nav"[^>]*>([\s\S]*?)<\/nav>/)?.[1];
  const topLabels = [...(topNav?.matchAll(/<a\b[^>]*>([\s\S]*?)<\/a>/g) || [])]
    .map(([, label]) => label.trim());
  for (const level of ["Guides", "Docs", "Specs"]) {
    assert.ok(topLabels.includes(level), `${route}: missing ${level} top navigation`);
  }
  const pager = page.match(/<nav class="page-links"[^>]*>([\s\S]*?)<\/nav>/)?.[1];
  assert.ok(pager, `${route}: missing previous/next navigation`);
  const neighbors = {};
  for (const [, attrs] of pager.matchAll(/<a\b([^>]*)>/g)) {
    const direction = attrs.match(/rel="(prev|next)"/)?.[1];
    const href = attrs.match(/href="([^"]+)"/)?.[1];
    assert.ok(direction && href, `${route}: page link needs a direction and destination`);
    assert.ok(!neighbors[direction], `${route}: duplicate ${direction} link`);
    const target = href.slice(base.length);
    assert.ok(allowed.some((prefix) => target === prefix || target.startsWith(`${prefix}/`)),
      `${route}: ${direction} link leaves ${selected}: ${href}`);
    neighbors[direction] = target;
  }
  const current = `/${route.replace(/\/index\.html$/, "")}`;
  pagination.set(current === "/spec/terminology" ? "/terminology" : current, neighbors);
}
for (const [current, neighbors] of pagination) {
  if (neighbors.next) assert.equal(pagination.get(neighbors.next)?.prev, current, `${current}: next link does not lead back`);
  if (neighbors.prev) assert.equal(pagination.get(neighbors.prev)?.next, current, `${current}: previous link does not lead forward`);
}
for (const [first, last] of levelBoundaries.values()) {
  assert.equal(pagination.get(first)?.prev, undefined, `${first}: first page must not have a previous link`);
  assert.ok(pagination.get(first)?.next, `${first}: missing next page`);
  assert.equal(pagination.get(last)?.next, undefined, `${last}: last page must not have a next link`);
  assert.ok(pagination.get(last)?.prev, `${last}: missing previous page`);
}
for (const route of ["guides/getting-started", "docs/records-and-sync", "docs/privacy-and-circles"]) {
  assert.ok(existsSync(resolve(dist, route, "index.html")), `Missing documentation level page: ${route}`);
}
assert.equal(failures.length, 0, failures.join("\n"));
console.log(`Checked ${html.length} HTML pages, ${links} local links, all raw specification assets, and search output (base ${base || "/"}).`);
