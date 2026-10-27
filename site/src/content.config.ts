import { defineCollection, z } from "astro:content";
import { glob } from "astro/loaders";

const spec = defineCollection({
  loader: glob({ pattern: "[0-9][0-9]-*.md", base: "../docs/spec" }),
  schema: z.object({
    title: z.string(),
    slug: z.string(),
    order: z.number(),
    group: z.string(),
  }),
});

const articleSchema = z.object({
  title: z.string(),
  slug: z.string(),
  description: z.string(),
  order: z.number(),
});

const guides = defineCollection({
  loader: glob({ pattern: "*.md", base: "../docs/guides" }),
  schema: articleSchema,
});

const docs = defineCollection({
  loader: glob({ pattern: "*.md", base: "../docs/features" }),
  schema: articleSchema,
});

export const collections = { guides, docs, spec };
