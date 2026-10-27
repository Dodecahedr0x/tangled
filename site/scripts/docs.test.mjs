import assert from "node:assert/strict";
import { test } from "node:test";
import { resolve } from "node:path";
import { docRoute, docsRoot, llmsText, rewriteDocLink, sourceRoute, specFiles } from "./docs.mjs";
import remarkDocs from "./remark-docs.mjs";

const architecture = resolve(docsRoot, "spec/03-architecture.md");

test("chapter, guide, and machine-asset links preserve fragments under a Pages base", () => {
  assert.equal(rewriteDocLink("./04-identifiers.md#rotation", architecture, "/tangled"), "/tangled/spec/identifiers#rotation");
  assert.equal(rewriteDocLink("./01-terminology.md", architecture, "/tangled"), "/tangled/terminology");
  assert.equal(rewriteDocLink("../features/social-imports.md#privacy", architecture, "/tangled"), "/tangled/docs/social-imports#privacy");
  assert.equal(rewriteDocLink("../guides/getting-started.md", architecture, "/tangled"), "/tangled/guides/getting-started");
  assert.equal(rewriteDocLink("./http/openapi.yaml", architecture, "/tangled"), "/tangled/spec/http/openapi.yaml");
  assert.equal(rewriteDocLink("/reference/http", architecture, "/tangled"), "/tangled/reference/http");
  assert.equal(docRoute("index"), "/docs");
  assert.equal(docRoute("index", "guides"), "/guides");
  assert.equal(docRoute("comparison", "guides"), "/guides/comparison");
  assert.equal(sourceRoute(resolve(docsRoot, "features/index.md")), "/docs");
});

test("external and same-page links are preserved; broken local links fail loudly", () => {
  for (const url of ["https://example.com/spec.md", "#actors", "mailto:hello@example.com", "//example.com"]) {
    assert.equal(rewriteDocLink(url, architecture), url);
  }
  assert.throws(() => rewriteDocLink("./missing.md", architecture), /Broken documentation link/);
  assert.equal(rewriteDocLink("../../crates/tangled/README.md", resolve(docsRoot, "spec/00-abstract.md")),
    "https://github.com/Dodecahedr0x/tangled/blob/main/crates/tangled/README.md");
});

test("Mermaid fences become escaped diagram containers, ordinary code stays code", () => {
  const diagram = { type: "code", lang: "mermaid", meta: 'A "caption"', value: 'A[<script>] --> B' };
  const code = { type: "code", lang: "sh", value: "echo hello" };
  remarkDocs()({ type: "root", children: [diagram, code] }, { path: architecture });
  assert.equal(diagram.type, "html");
  assert.match(diagram.value, /class="mermaid"/);
  assert.match(diagram.value, /&lt;script&gt;/);
  assert.match(diagram.value, /A &quot;caption&quot;/);
  assert.equal(code.type, "code");
});

test("downloads and LLM text contain canonical sources and exclude internal/dependency files", () => {
  const files = specFiles();
  assert.ok(files.some((file) => file.endsWith("cddl/record.cddl")));
  assert.ok(files.some((file) => file.endsWith("vectors/mls-welcome-commit.json")));
  assert.ok(files.every((file) => !file.includes("node_modules") && !file.includes("superpowers")));
  const llms = llmsText();
  assert.match(llms, /# docs\/guides\/getting-started.md/);
  assert.match(llms, /# docs\/features\/social-imports.md/);
  assert.match(llms, /# docs\/spec\/00-abstract.md/);
  assert.doesNotMatch(llms, /# docs\/(?:internal|superpowers)\//);
  assert.equal(sourceRoute(resolve(docsRoot, "internal/go-to-market.md")), undefined);
});
