import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, relative, resolve, sep } from "node:path";

// npm runs the site commands with site/ as cwd. Do not use import.meta.url:
// Astro relocates bundled endpoint modules into dist/ during prerendering.
export const repoRoot = resolve(process.cwd(), "..");
export const docsRoot = resolve(repoRoot, "docs");

export function specFiles(directory = resolve(docsRoot, "spec")) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    if (entry.name.startsWith(".") || entry.name === "node_modules") return [];
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? specFiles(path) : [path];
  });
}

export function llmsText() {
  const articles = ["guides", "features"].flatMap((directory) => readdirSync(resolve(docsRoot, directory))
    .filter((name) => name.endsWith(".md")).sort().map((name) => resolve(docsRoot, directory, name)));
  const chapters = specFiles().filter((path) => /[/\\]\d{2}-.+\.md$/.test(path)).sort();
  const files = [...articles, ...chapters];
  return "# Tangled documentation\n\nGuides introduce Tangled and help you get started. Docs explain feature behavior and implementation boundaries. Specs define the formal version 1 protocol. The documentation site is static; provider and app programs run separately.\n" + files.map((path) =>
    `\n\n---\n\n# docs/${relative(docsRoot, path).split(sep).join("/")}\n\n${readFileSync(path, "utf8")}`
  ).join("");
}

// Preserve the established public URLs while keeping the sources in docs/.
export function docRoute(slug, level = "features") {
  const prefix = level === "guides" ? "/guides" : "/docs";
  return slug === "index" ? prefix : `${prefix}/${slug}`;
}

export function sourceRoute(file) {
  const path = relative(docsRoot, file).split(sep).join("/");
  const chapter = path.match(/^spec\/\d{2}-([a-z0-9-]+)\.md$/);
  if (chapter) return chapter[1] === "terminology" ? "/terminology" : `/spec/${chapter[1]}`;
  if (path.startsWith("spec/")) return `/${path}`;
  const article = path.match(/^(guides|features)\/([a-z0-9-]+)\.md$/);
  if (article) return docRoute(article[2], article[1]);
  return undefined;
}

export function rewriteDocLink(url, source, base = "/") {
  if (!url || /^(?:[a-z][a-z\d+.-]*:|#|\/\/)/i.test(url)) return url;
  const prefix = base.replace(/\/$/, "");
  if (url.startsWith("/")) return `${prefix}${url}`;
  const [, pathname, suffix] = url.match(/^([^?#]*)(.*)$/);
  const target = resolve(dirname(source), decodeURI(pathname));
  if (!existsSync(target)) throw new Error(`Broken documentation link in ${source}: ${url}`);
  const route = sourceRoute(target);
  if (route) return `${prefix}${route}${suffix}`;
  // Source-code links outside the published collection remain useful on the site.
  const repoPath = relative(repoRoot, target).split(sep).join("/");
  if (repoPath.startsWith("../")) throw new Error(`Link leaves repository: ${url}`);
  return `https://github.com/Dodecahedr0x/tangled/blob/main/${repoPath}${suffix}`;
}
