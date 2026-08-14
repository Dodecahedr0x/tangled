import { defineCollection, z } from "astro:content";
import { glob } from "astro/loaders";

const spec = defineCollection({
  loader: glob({ pattern: "[0-9][0-9]-*.md", base: "../spec" }),
  schema: z.object({
    title: z.string(),
    slug: z.string(),
    order: z.number(),
  }),
});

export const collections = { spec };
