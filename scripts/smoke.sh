#!/usr/bin/env bash
# Checks a deployed Split Card site end to end:
#   scripts/smoke.sh https://yourdomain [team-slug]
#
# Each step prints PASS, FAIL or SKIP; the exit status is 1 if any failed.
# The signed step uses a fresh throwaway key and leaves one profile named
# "Smoke test" behind. Needs cargo (for the NIP-98 signer), curl and python3.
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "usage: $0 <https://yourdomain> [team-slug]" >&2
  exit 2
fi
base=${1%/}
slug=${2:-}

root=$(cd "$(dirname "$0")/.." && pwd)
cargo build -q --manifest-path "$root/Cargo.toml" -p splitcard-server --example nip98
nip98="$root/target/debug/examples/nip98"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

failures=0
pass() { echo "PASS  $1"; }
fail() { echo "FAIL  $1: $2"; failures=$((failures + 1)); }
skip() { echo "SKIP  $1: $2"; }

# Prints the status code; the body is left in $work/body.
fetch() {
  curl -sS -o "$work/body" -w '%{http_code}' --max-time 20 "$@" || echo 000
}
json_field() {
  python3 -I -c 'import json,sys
try: print(json.load(open(sys.argv[1]))[sys.argv[2]])
except Exception: print("")' "$work/body" "$1"
}

# 1. The website is served.
status=$(fetch "$base/")
if [[ $status == 200 ]] && grep -qi '<html' "$work/body"; then
  pass "website served at $base/"
else
  fail "website served at $base/" "status $status"
fi

# 2. The API answers through the proxy.
status=$(fetch "$base/api/health")
if [[ $status == 200 && $(json_field status) == ok ]]; then
  pass "GET /api/health"
else
  fail "GET /api/health" "status $status, body $(head -c 200 "$work/body")"
fi

# 3. Lightning Address error shape for an unknown team.
skip "GET /.well-known/lnurlp/<unknown>" "LNURL endpoints are not available yet"

# 4. A NIP-98 signed write, checked against the public URL.
key=$("$nip98" keygen)
pubkey=$(NIP98_SECRET_KEY=$key "$nip98" pubkey)
printf '{"name":"Smoke test","lightning_address":"smoke@example.com"}' >"$work/profile.json"
url="$base/api/me/profile"
auth=$(NIP98_SECRET_KEY=$key "$nip98" sign PUT "$url" "$work/profile.json")
status=$(fetch -X PUT "$url" -H "Authorization: $auth" -H 'Content-Type: application/json' \
  --data-binary @"$work/profile.json")
if [[ $status == 200 && $(json_field pubkey) == "$pubkey" ]]; then
  pass "signed PUT /api/me/profile (throwaway key $pubkey)"
else
  fail "signed PUT /api/me/profile" "status $status, body $(head -c 300 "$work/body")"
fi

# 5. The team's Lightning Address.
if [[ -n $slug ]]; then
  skip "probe $slug's Lightning Address" "LNURL endpoints are not available yet"
else
  skip "probe a team's Lightning Address" "no team slug given, and LNURL is not available yet"
fi

if ((failures > 0)); then
  echo "$failures step(s) failed"
  exit 1
fi
echo "smoke test passed"
