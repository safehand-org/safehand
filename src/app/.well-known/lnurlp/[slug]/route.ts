// `/.well-known/lnurlp/:slug` → the same path on the server, for team
// Lightning Addresses. See `@/lib/serverProxy`.
import { proxyToServer } from "@/lib/serverProxy";

function handler(request: Request): Promise<Response> {
  return proxyToServer(request);
}

export { handler as GET, handler as HEAD, handler as OPTIONS };
