import { mkdirSync, readdirSync, readFileSync, writeFileSync, copyFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "../..");
const spec = join(root, "spec");
const pub = join(root, "site/public");
mkdirSync(join(pub, "spec"), { recursive: true });

copyFileSync(join(spec, "http/openapi.yaml"), join(pub, "openapi.yaml"));

const chapters = readdirSync(spec)
  .filter((f) => /^\d{2}-.+\.md$/.test(f))
  .sort();

let llms = `# Tangled protocol v1\n\nCanonical specification. This is not a live network.\n\n`;
for (const f of chapters) {
  const body = readFileSync(join(spec, f), "utf8");
  writeFileSync(join(pub, "spec", f), body);
  llms += `\n\n---\n\n# ${f}\n\n${body}`;
}
writeFileSync(join(pub, "llms.txt"), llms);
console.log("copied", chapters.length, "chapters");
