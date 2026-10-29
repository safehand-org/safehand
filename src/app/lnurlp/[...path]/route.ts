// `/lnurlp/:path*` → the same path on the server, for LNURL-pay callbacks
// and verify URLs. See `@/lib/serverProxy`.
import { proxyToServer } from "@/lib/serverProxy";

function handler(request: Request): Promise<Response> {
  return proxyToServer(request);
}

export { handler as GET, handler as HEAD, handler as OPTIONS };
