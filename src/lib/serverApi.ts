// Unsigned reads of the Split Card server from server components, over the
// private network. Only public data: anything for a particular member is
// signed in the browser instead (`@/lib/api`).

import { serverBase } from "./serverProxy";

export type ServerRead<T> =
  | { ok: true; data: T }
  | { ok: false; status: number; code: string | null };

export async function serverGet<T>(path: string): Promise<ServerRead<T>> {
  const base = serverBase();
  if (!base) return { ok: false, status: 503, code: "server_not_configured" };
  let response: Response;
  try {
    response = await fetch(`${base}${path}`, { cache: "no-store" });
  } catch {
    return { ok: false, status: 502, code: "server_unavailable" };
  }
  const data: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const code = (data as { code?: unknown } | null)?.code;
    return { ok: false, status: response.status, code: typeof code === "string" ? code : null };
  }
  return { ok: true, data: data as T };
}
