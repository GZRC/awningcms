#!/usr/bin/env bash
# Dumps this installation's PostgreSQL database and keeps 14 days of dumps.
set -euo pipefail
[[ $EUID -eq 0 ]] || { echo 'Lancez ce script avec sudo.' >&2; exit 1; }
cd "$(dirname "$0")/.."
backup_dir=${BACKUP_DIR:-/var/backups/awningcms}
install -d -m 700 "$backup_dir"
umask 077
backup="$backup_dir/cms-$(date -u +%Y%m%dT%H%M%SZ).dump"
trap 'rm -f "${backup}.part"' EXIT
files=(-f infra/compose.yaml)
[[ -f infra/compose.override.yaml ]] && files+=(-f infra/compose.override.yaml)
docker compose --env-file infra/.env "${files[@]}" exec -T postgres \
    pg_dump -U cms -d cms -Fc > "${backup}.part"
test -s "${backup}.part"
mv "${backup}.part" "$backup"
# Rotation only touches dumps made by this script, after a successful dump.
find "$backup_dir" -maxdepth 1 -type f -name 'cms-*.dump' -mtime +14 -delete
echo "Sauvegarde PostgreSQL créée : $backup"
