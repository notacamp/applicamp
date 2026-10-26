#!/bin/sh
# Prepares the `server-secrets` volume. Runs as a one-shot container before
# `server` on every `docker compose up`.
#
# Once, on the very first run, it generates `secret-key`
# (APPLICAMP_SECRET_KEY_FILE): 32 random bytes, base64. The server seals the
# sign-in cookie and stored refresh tokens with keys derived from it.
# Replacing it signs every sign-in in progress out and makes stored refresh
# tokens unreadable (people sign in again).
#
# The server mounts the volume read-only and reads the key through its
# supplementary group 10001 (the file is 0440).
set -eu
umask 077

DIR=/secrets
OWNER=10001

mkdir -p "$DIR"
if [ ! -s "$DIR/secret-key" ]; then
  head -c 32 /dev/urandom | base64 >"$DIR/secret-key.tmp"
  mv "$DIR/secret-key.tmp" "$DIR/secret-key"
  echo "server-init: generated the instance secret key"
fi

# 0750 directory and a group-readable key for the server.
chown "$OWNER:$OWNER" "$DIR" "$DIR/secret-key"
chmod 0750 "$DIR"
chmod 0440 "$DIR/secret-key"
