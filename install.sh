#!/usr/bin/env bash
# Installs or updates AwningCMS on this server: secrets, settings, image and
# containers. Safe to run again: existing secrets and data are kept.
#
# Answers can be given in advance (for automation):
#   AWNING_ADMIN_HOST, AWNING_SITE_HOST, AWNING_PUBLIC_IP,
#   AWNING_HTTP_PORT, AWNING_HTTPS_PORT, AWNING_PROJECT, AWNING_YES=1
set -euo pipefail
cd "$(dirname "$0")"
root=$PWD
settings=infra/.env

fail() { echo "Erreur : $*" >&2; exit 1; }

if ! { command -v docker && docker compose version; } >/dev/null 2>&1; then
    fail "Docker et Docker Compose sont nécessaires : https://docs.docker.com/engine/install/"
fi
[[ $EUID -eq 0 ]] || fail "lancez ce script avec sudo : il protège les fichiers secrets."

# Previous answers, used as defaults when the script runs again.
previous() { [[ -f $settings ]] && sed -n "s/^$1=//p" "$settings" | tail -n 1 || true; }

ask() {
    local question=$2 default=$3 value=${!1:-}
    if [[ -z $value && ${AWNING_YES:-} != 1 ]]; then
        read -r -p "$question${default:+ [$default]} : " value
    fi
    printf '%s' "${value:-$default}"
}

valid_host() {
    [[ $1 =~ ^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$ ]]
}

detect_ip() {
    ip -4 route get 1.1.1.1 2>/dev/null | sed -n 's/.* src \([0-9.]*\).*/\1/p' | head -n 1
}

admin_host=$(ask AWNING_ADMIN_HOST "Adresse de l'administration (ex. cms.exemple.fr)" "$(previous ADMIN_HOST)")
site_host=$(ask AWNING_SITE_HOST "Adresse du site principal (ex. www.exemple.fr)" "$(previous SITE_HOST)")
admin_host=${admin_host,,}
site_host=${site_host,,}
valid_host "$admin_host" || fail "adresse de l'administration invalide : « $admin_host »."
valid_host "$site_host" || fail "adresse du site invalide : « $site_host »."
ip_default=$(previous PUBLIC_IPS)
public_ip=$(ask AWNING_PUBLIC_IP "Adresse IP publique de ce serveur" "${ip_default:-$(detect_ip)}")
# One address, or IPv4 and IPv6 separated by a comma.
[[ $public_ip =~ ^[0-9a-fA-F.:]+(,[0-9a-fA-F.:]+)?$ ]] || fail "adresse IP invalide : « $public_ip »."
# Ports are only changed on purpose (another web server on this machine).
http_port=${AWNING_HTTP_PORT:-$(previous HTTP_PORT)}
https_port=${AWNING_HTTPS_PORT:-$(previous HTTPS_PORT)}
http_port=${http_port:-80}
https_port=${https_port:-443}
project=${AWNING_PROJECT:-$(previous COMPOSE_PROJECT_NAME)}
project=${project:-awningcms}
[[ $http_port =~ ^[0-9]+$ && $https_port =~ ^[0-9]+$ ]] || fail "les ports doivent être des nombres."
[[ $project =~ ^[a-z0-9][a-z0-9_-]*$ ]] || fail "nom de projet invalide : « $project »."

origin() { [[ $https_port == 443 ]] && echo "https://$1" || echo "https://$1:$https_port"; }
caddyfile=Caddyfile
[[ $admin_host == "$site_host" ]] && caddyfile=Caddyfile.one-host

# Secrets: created once, never replaced. PostgreSQL runs as UID 70 and the
# application as UID 10001 inside their containers.
install -d -m 700 private
new_secret() {
    local file=private/$1.secret
    if [[ ! -s $file ]]; then
        (umask 077; head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' > "$file"; echo >> "$file")
    fi
    chown "$2" "$file"
    chmod "$3" "$file"
}
new_secret db-password 70:70 400
new_secret app-db-password 10001:70 440
new_secret setup-token 10001:10001 400

version=$(git -C "$root" rev-parse --short HEAD 2>/dev/null || echo local)
image=awningcms:$version
cat > "$settings" <<SETTINGS
# Written by install.sh. Run it again to change these values.
COMPOSE_PROJECT_NAME=$project
ADMIN_HOST=$admin_host
SITE_HOST=$site_host
PUBLIC_ORIGIN=$(origin "$admin_host")
PRIMARY_SITE_ORIGIN=$(origin "$site_host")
PUBLIC_IPS=$public_ip
HTTP_PORT=$http_port
HTTPS_PORT=$https_port
CADDYFILE=$caddyfile
AWNINGCMS_IMAGE=$image
SETTINGS

echo "Construction de l'image $image (plusieurs minutes la première fois)…"
docker build -t "$image" "$root"
# Local additions (extra domains, for example) go in infra/compose.override.yaml.
files=(-f "$root/infra/compose.yaml")
[[ -f infra/compose.override.yaml ]] && files+=(-f "$root/infra/compose.override.yaml")
docker compose --env-file "$root/$settings" "${files[@]}" up -d --wait

cat <<DONE

AwningCMS fonctionne.
1. Vérifiez que $admin_host et $site_host pointent vers $public_ip.
2. Ouvrez $(origin "$admin_host")/admin/setup pour créer le compte administrateur.
   Le code de configuration se lit avec :
   sudo cat $root/private/setup-token.secret
DONE
