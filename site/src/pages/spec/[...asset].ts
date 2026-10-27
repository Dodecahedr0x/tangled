import { readFileSync } from "node:fs";
import { relative, resolve } from "node:path";
import { docsRoot, specFiles } from "../../../scripts/docs.mjs";
import type { APIRoute } from "astro";

export function getStaticPaths() {
  return specFiles().map((path) => ({
    params: { asset: relative(resolve(docsRoot, "spec"), path).replaceAll("\\", "/") },
    props: { path },
  }));
}

export const GET: APIRoute = ({ props }) => new Response(readFileSync(props.path), {
  headers: { "Content-Type": props.path.endsWith(".json") ? "application/json" : "text/plain; charset=utf-8" },
});
