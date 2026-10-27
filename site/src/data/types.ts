export interface TypeDoc {
  name: string;
  module: string;
  signature: string;
  summary: string;
  href: string;
  fields: { name: string; type: string; note: string }[];
  impl: string[];
  see: string;
}

import data from "../../../docs/reference/types.json";

export const types: TypeDoc[] = data;
