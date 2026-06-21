#!/bin/sh
set -e
# An empty host bind-mount at /data lands root:root; make it writable by the
# unprivileged runtime user, then drop privileges. A no-op (|| true) when the
# caller already passed --user.
chown -R appuser:appuser /data 2>/dev/null || true
exec gosu appuser shirita-web "$@"
