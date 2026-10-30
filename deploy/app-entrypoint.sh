#!/usr/bin/env bash
# Runs the Split Card API server and the website in one container
# (deploy/app.Dockerfile). The server listens only inside the container; the
# website listens on $PORT and forwards /api to it.
#
# If either process exits, the other is stopped and the container exits
# with a failure, so the platform restarts it. SIGTERM is passed to both;
# the server finishes in-flight work before it exits.
set -euo pipefail

server_port=${SPLITCARD_SERVER_PORT:-4000}

# The server reads PORT before BIND_ADDR; here PORT belongs to the website.
# server-entrypoint hands the disk to the splitcard user, drops root and
# starts the server, under Litestream when it is configured.
env -u PORT BIND_ADDR="127.0.0.1:$server_port" server-entrypoint &
server=$!

setpriv --reuid=node --regid=node --init-groups \
  env SERVER_INTERNAL_URL="127.0.0.1:$server_port" node /app/server.js &
web=$!

stopping=0
stop() {
  stopping=1
  kill -TERM "$web" "$server" 2>/dev/null || true
}
trap stop TERM INT

# Wait for the first exit (or a signal), then stop the other process.
wait -n "$server" "$web" || true
if [[ $stopping == 0 ]]; then
  echo "a process exited unexpectedly; stopping the container" >&2
fi
stop
server_status=0
wait "$server" || server_status=$?
wait "$web" || true
if [[ $stopping == 1 && $server_status == 0 ]]; then
  exit 0
fi
exit 1
