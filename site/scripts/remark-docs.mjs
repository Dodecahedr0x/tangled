import { rewriteDocLink } from "./docs.mjs";

function walk(node, visit) {
  visit(node);
  if (Array.isArray(node.children)) node.children.forEach((child) => walk(child, visit));
}

const escape = (value) => value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

export default function remarkDocs({ base = "/" } = {}) {
  return (tree, file) => walk(tree, (node) => {
    if (node.type === "link" || node.type === "definition" || node.type === "image") {
      node.url = rewriteDocLink(node.url, file.path, base);
    }
    if (node.type === "code" && node.lang === "mermaid") {
      const label = node.meta || "Diagram";
      node.type = "html";
      node.value = `<figure class="diagram" aria-label="${escape(label)}"><pre class="mermaid">${escape(node.value)}</pre><figcaption>${escape(label)}</figcaption></figure>`;
    }
  });
}
