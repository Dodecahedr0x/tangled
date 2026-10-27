export interface HttpMethod {
  id: string;
  method: "GET" | "POST";
  path: string;
  summary: string;
  auth: string;
  params: { name: string; in: string; required: boolean; note: string }[];
  request?: string;
  response: string;
}

import data from "../../../docs/reference/http.json";

export const httpMethods: HttpMethod[] = data as HttpMethod[];
