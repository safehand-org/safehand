// Forwards requests to the Split Card server on Render's private network.
//
// NIP-98 signs the exact URL, method and body, so nothing here may change
// them: the path and query are passed on as received, the body is streamed
// byte for byte, and the Authorization header is forwarded untouched. The
// response's status, headers and body come back unchanged.

// Headers that describe one connection, not the request or response.
const HOP_BY_HOP = new Set([
  "connection",
  "keep-alive",
  "proxy-authenticate",
  "proxy-authorization",
  "proxy-connection",
  "te",
  "trailer",
  "transfer-encoding",
  "upgrade",
]);

/** The server's base URL from SERVER_INTERNAL_URL, which may be a bare host:port. */
function serverBase(): string | null {
  const configured = process.env.SERVER_INTERNAL_URL?.trim();
  if (!configured) return null;
  const withScheme = /^https?:\/\//.test(configured) ? configured : `http://${configured}`;
  return withScheme.replace(/\/+$/, "");
}

function errorResponse(status: number, code: string, message: string): Response {
  return Response.json({ code, message, details: null }, { status });
}

/**
 * Proxies `request` to the server. `stripPrefix` is removed from the start
 * of the path (`/api` for the API); the rest of the path and the query
 * string are kept exactly as the browser sent them.
 */
export async function proxyToServer(request: Request, stripPrefix = ""): Promise<Response> {
  const base = serverBase();
  if (!base) {
    return errorResponse(503, "server_not_configured", "SERVER_INTERNAL_URL is not set");
  }
  // Read the raw request target so percent-encoding is never re-normalized.
  const url = new URL(request.url);
  let path = url.pathname;
  if (stripPrefix && path.startsWith(stripPrefix)) {
    path = path.slice(stripPrefix.length) || "/";
  }

  const headers = new Headers();
  request.headers.forEach((value, name) => {
    const lower = name.toLowerCase();
    if (HOP_BY_HOP.has(lower) || lower === "host" || lower === "content-length") return;
    headers.append(name, value);
  });

  const hasBody = request.method !== "GET" && request.method !== "HEAD";
  let upstream: Response;
  try {
    upstream = await fetch(`${base}${path}${url.search}`, {
      method: request.method,
      headers,
      body: hasBody ? request.body : undefined,
      // Stream the body as received; required by Node's fetch for streams.
      duplex: "half",
      redirect: "manual",
      cache: "no-store",
    } as RequestInit & { duplex: "half" });
  } catch {
    return errorResponse(502, "server_unavailable", "the Split Card server could not be reached");
  }

  const responseHeaders = new Headers();
  upstream.headers.forEach((value, name) => {
    if (!HOP_BY_HOP.has(name.toLowerCase())) responseHeaders.append(name, value);
  });
  // Node's fetch decodes compressed bodies; their encoding and length no
  // longer describe what is sent on. The server does not compress today.
  if (responseHeaders.has("content-encoding")) {
    responseHeaders.delete("content-encoding");
    responseHeaders.delete("content-length");
  }
  return new Response(request.method === "HEAD" ? null : upstream.body, {
    status: upstream.status,
    statusText: upstream.statusText,
    headers: responseHeaders,
  });
}
