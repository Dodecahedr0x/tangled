export const githubUrl = "https://github.com/tangled-protocol/tangled";

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
  { href: "/", label: "Landing", section: "landing" as const },
  { href: "/docs", label: "Docs", section: "docs" as const },
  { href: "/reference/http", label: "Reference", section: "reference" as const },
];

export function sectionFor(path: string): "landing" | "docs" | "reference" {
  if (path === "/" || path === "") return "landing";
  if (path.startsWith("/reference")) return "reference";
  return "docs";
}

export const specNav = [
  {
    module: "spec::intro",
    items: [
      { href: "/spec/abstract", label: "Abstract" },
      { href: "/terminology", label: "Terminology" },
      { href: "/spec/conformance", label: "Conformance" },
      { href: "/spec/architecture", label: "Architecture" },
    ],
  },
  {
    module: "spec::identity",
    items: [
      { href: "/spec/identifiers", label: "Identifiers" },
      { href: "/spec/cryptography", label: "Cryptography" },
      { href: "/spec/resolution", label: "Resolution" },
    ],
  },
  {
    module: "spec::data",
    items: [
      { href: "/spec/data-model", label: "Data model" },
      { href: "/spec/repositories", label: "Repositories" },
      { href: "/spec/visibility", label: "Visibility" },
    ],
  },
  {
    module: "spec::auth",
    items: [
      { href: "/spec/capabilities", label: "Capabilities" },
      { href: "/spec/circles", label: "Circles" },
    ],
  },
  {
    module: "spec::wire",
    items: [
      { href: "/spec/sync", label: "Synchronization" },
      { href: "/spec/http-api", label: "HTTP API" },
    ],
  },
  {
    module: "spec::network",
    items: [
      { href: "/spec/looms", label: "Looms" },
      { href: "/spec/labels", label: "Labels" },
      { href: "/spec/interoperability", label: "Interoperability" },
      { href: "/spec/migration", label: "Migration" },
    ],
  },
  {
    module: "spec::eval",
    items: [
      { href: "/spec/use-cases", label: "Use cases" },
      { href: "/spec/threat-model", label: "Threat model" },
      { href: "/spec/comparison", label: "Comparison" },
      { href: "/spec/changelog", label: "Changelog" },
    ],
  },
];

export const toolNav = [
  { href: "/reference/types", label: "Type index" },
  { href: "/reference/http", label: "HTTP reference" },
  { href: "/reference/openapi", label: "OpenAPI" },
  { href: "/compare", label: "Compare" },
  { href: "/interop", label: "Interop tables" },
  { href: "/playground", label: "Playground" },
];
