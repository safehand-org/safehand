#!/usr/bin/env bash
# Sends signed requests through the website's /api proxy to a real server and
# checks that the server accepts them: NIP-98 signs the exact URL, method and
# body, so any change the proxy makes shows up as a 401.
#
# Needs `npm run build` first (for .next/standalone) plus cargo, curl and
# python3. Run with `npm run test:proxy`.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

if [[ ! -f .next/standalone/server.js ]]; then
  echo "no .next/standalone/server.js; run npm run build first" >&2
  exit 2
fi

cargo build -q -p splitcard-server --bin splitcard-server --example nip98
server_bin=target/debug/splitcard-server
nip98=target/debug/examples/nip98

work=$(mktemp -d)
pids=()
cleanup() {
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

free_port() {
  python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'
}
server_port=$(free_port)
web_port=$(free_port)
web="http://127.0.0.1:$web_port"
server="http://127.0.0.1:$server_port"

SPLITCARD_DEV=1 \
  DATABASE_URL="$work/splitcard.db" \
  PUBLIC_BASE_URL="$web/api" \
  BIND_ADDR="127.0.0.1:$server_port" \
  RELAYS=wss://relay.invalid \
  SERVER_SECRET_KEY="$("$nip98" keygen)" \
  NO_COLOR=1 \
  "$server_bin" >"$work/server.log" 2>&1 &
pids+=($!)

# A bare host:port, as Render's fromService hostport gives it.
PORT=$web_port HOSTNAME=127.0.0.1 SERVER_INTERNAL_URL="127.0.0.1:$server_port" \
  node .next/standalone/server.js >"$work/web.log" 2>&1 &
pids+=($!)

for _ in $(seq 1 100); do
  if curl -sf "$web/api/health" >/dev/null 2>&1; then break; fi
  sleep 0.1
done

failures=0
check() {
  local name=$1 expected=$2 actual=$3
  if [[ "$actual" == "$expected" ]]; then
    echo "PASS  $name"
  else
    echo "FAIL  $name: expected $expected, got $actual"
    failures=$((failures + 1))
  fi
}

# status and body of a request, as "<status> <body>"
request() {
  local out
  out=$(curl -s -o "$work/body" -w '%{http_code}' "$@")
  echo "$out $(cat "$work/body")"
}

check "health through the proxy" '200 {"status":"ok"}' "$(request "$web/api/health")"

key=$("$nip98" keygen)
pubkey=$(NIP98_SECRET_KEY=$key "$nip98" pubkey)

# Not what a JSON serializer would write: extra spaces, a \u escape and no
# trailing newline. Any re-encoding changes the payload hash.
printf '{ "name" :  "\\u00c5lice",\n  "lightning_address":"alice@blink.sv" }' >"$work/profile.json"
url="$web/api/me/profile?via=proxy&x=%2Fa+b"
auth=$(NIP98_SECRET_KEY=$key "$nip98" sign PUT "$url" "$work/profile.json")
put() {
  request -X PUT "$url" -H "Authorization: $1" -H 'Content-Type: application/json' \
    --data-binary @"$2"
}

result=$(put "$auth" "$work/profile.json")
check "signed PUT accepted" 200 "${result%% *}"
check "body arrived unchanged" '"Ålice"' "$(echo "${result#* }" | python3 -I -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["name"], ensure_ascii=False))')"

result=$(put "$auth" "$work/profile.json")
check "replayed header rejected" "401 auth_replayed" "${result%% *} $(echo "${result#* }" | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["code"])')"

printf '{"name":"Mallory","lightning_address":"alice@blink.sv"}' >"$work/other.json"
fresh=$(NIP98_SECRET_KEY=$key "$nip98" sign PUT "$url" "$work/profile.json")
result=$(put "$fresh" "$work/other.json")
check "changed body rejected" "401 auth_payload_mismatch" "${result%% *} $(echo "${result#* }" | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["code"])')"

get_url="$web/api/me"
result=$(request "$get_url" -H "Authorization: $(NIP98_SECRET_KEY=$key "$nip98" sign GET "$get_url")")
check "signed GET accepted" 200 "${result%% *}"

# Errors pass through exactly as the server sends them.
for path in "/profiles/$pubkey" "/invites/no%20such%2Fslug" "/teams/by-slug/missing?x=1" "/me" "/nope"; do
  direct=$(request "$server$path")
  proxied=$(request "$web/api$path")
  check "same response for $path" "$direct" "$proxied"
done

if ((failures > 0)); then
  echo "$failures check(s) failed; server log:"
  cat "$work/server.log"
  exit 1
fi
echo "all proxy checks passed"
