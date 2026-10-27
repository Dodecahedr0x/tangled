import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { docsRoot } from "../../scripts/docs.mjs";

export function GET() {
  return new Response(readFileSync(resolve(docsRoot, "spec/http/openapi.yaml")), {
    headers: { "Content-Type": "application/yaml; charset=utf-8" },
  });
}
