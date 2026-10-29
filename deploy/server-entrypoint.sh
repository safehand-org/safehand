#!/bin/sh
# Starts the Split Card server. With LITESTREAM_REPLICA_URL and credentials
# set, the database is restored from the replica when the disk has none,
# then the server runs under `litestream replicate`; otherwise the server
# runs directly.
set -eu

user=splitcard
db_path=${DATABASE_URL#sqlite://}
db_path=${db_path#sqlite:}
db_path=${db_path%%\?*}
db_dir=$(dirname "$db_path")

# Started as root only to hand the mounted disk to the service user.
if [ "$(id -u)" = 0 ]; then
  if [ -d "$db_dir" ]; then
    chown -R "$user:$user" "$db_dir"
  fi
  exec setpriv --reuid="$user" --regid="$user" --init-groups "$0" "$@"
fi

replica=${LITESTREAM_REPLICA_URL:-}
key_id=${LITESTREAM_ACCESS_KEY_ID:-}
secret=${LITESTREAM_SECRET_ACCESS_KEY:-}
if [ -z "$replica" ] && [ -z "$key_id" ] && [ -z "$secret" ]; then
  echo "LITESTREAM_REPLICA_URL is not set; running without backups" >&2
  exec splitcard-server
fi
if [ -z "$replica" ] || [ -z "$key_id" ] || [ -z "$secret" ]; then
  echo "startup error: set all of LITESTREAM_REPLICA_URL, LITESTREAM_ACCESS_KEY_ID" \
    "and LITESTREAM_SECRET_ACCESS_KEY, or none of them" >&2
  exit 2
fi

export LITESTREAM_DB_PATH="$db_path"
config=/etc/litestream.yml
if [ ! -d "$db_dir" ]; then
  echo "startup error: database directory $db_dir does not exist" >&2
  exit 2
fi
# Only on an empty disk; an existing database is never overwritten.
litestream restore -config "$config" -if-db-not-exists -if-replica-exists "$db_path"
exec litestream replicate -config "$config" -exec splitcard-server
