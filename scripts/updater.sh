#!/bin/sh
# Entrypoint of the `updater` compose service: runs scripts/auto-update.sh every
# 20s, so Settings → Updates works on any install with no host cron/launchd.
#
# It holds the docker socket but publishes no ports and takes no input except
# the marker files the backend drops in update-status/ ("check", "requested").
# The worst a compromised backend can do is ask it to install the newest
# release tag from GitHub.
#
# Compose resolves bind mounts like ./update-status against the project
# directory and hands that path to the daemon, so the repo must sit at its
# real host path in here too: it's mounted at /eunomia and symlinked to the
# host path read from this container's own mount table.
set -u

apk add --no-cache -q bash git coreutils findutils >/dev/null 2>&1 || { echo "updater: apk add failed, retrying on restart"; exit 1; }
git config --global --add safe.directory '*'

host="$(docker inspect "$HOSTNAME" --format '{{range .Mounts}}{{if eq .Destination "/eunomia"}}{{.Source}}{{end}}{{end}}' 2>/dev/null)"
[ -n "$host" ] || { echo "updater: can't see my own mounts (is the docker socket mounted?)"; exit 1; }
if [ "$host" != /eunomia ] && [ ! -e "$host" ]; then
  mkdir -p "$(dirname "$host")" && ln -s /eunomia "$host"
fi
owner="$(stat -c %u:%g /eunomia)"
mkdir -p /eunomia/update-status

while :; do
  (cd "$host" && bash scripts/auto-update.sh) >> /eunomia/update-status/update.log 2>&1
  # root in here; keep the checkout owned by whoever owns it on the host
  chown -R "$owner" /eunomia/.git /eunomia/.env /eunomia/update-status 2>/dev/null
  git -C /eunomia ls-files -z | (cd /eunomia && xargs -0 chown -h "$owner" 2>/dev/null)
  tail -n 500 /eunomia/update-status/update.log > /tmp/log && cat /tmp/log > /eunomia/update-status/update.log
  sleep 20
done
