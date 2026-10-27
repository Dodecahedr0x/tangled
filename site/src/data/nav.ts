export const githubUrl = "https://github.com/Dodecahedr0x/tangled";

/** Prefix a site-root path with Astro `base` (GitHub Pages serves this repo at `/tangled`). */
export function withBase(path: string): string {
  const raw = import.meta.env.BASE_URL;
  const base = raw.endsWith("/") ? raw.slice(0, -1) : raw;
  if (!path || path === "/") return base || "/";
  return `${base}${path.startsWith("/") ? path : `/${path}`}`;
}

/** Strip `base` so nav matching still uses `/docs`, `/spec/…`. */
export function sitePath(pathname: string): string {
  const raw = import.meta.env.BASE_URL;
  const base = raw.endsWith("/") ? raw.slice(0, -1) : raw;
  let p = pathname.replace(/\/$/, "") || "/";
  if (base && (p === base || p.startsWith(`${base}/`))) {
    p = p.slice(base.length) || "/";
  }
  return p;
}

export const siteNav = [
  { href: "/", label: "Home", section: "landing" as const },
  { href: "/guides", label: "Guides", section: "guides" as const },
  { href: "/docs", label: "Docs", section: "docs" as const },
  { href: "/spec/abstract", label: "Specs", section: "spec" as const },
];

export function sectionFor(path: string): "landing" | "guides" | "docs" | "spec" {
  if (path === "/" || path === "") return "landing";
  if (path === "/guides" || path.startsWith("/guides/")) return "guides";
  if (path === "/docs" || path.startsWith("/docs/")) return "docs";
  return "spec";
}

export const toolNav = [
  { href: "/reference/types", label: "Type index" },
  { href: "/reference/http", label: "HTTP reference" },
  { href: "/reference/openapi", label: "OpenAPI" },
  { href: "/playground", label: "Playground" },
];
