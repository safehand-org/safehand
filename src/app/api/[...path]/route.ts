// `/api/:path*` → the server's `/:path*`. See `@/lib/serverProxy`.
import { proxyToServer } from "@/lib/serverProxy";

function handler(request: Request): Promise<Response> {
  return proxyToServer(request, "/api");
}

export { handler as DELETE, handler as GET, handler as HEAD, handler as OPTIONS };
export { handler as PATCH, handler as POST, handler as PUT };
