# Installer et exploiter AwningCMS

## Ce qu'il faut

- Un serveur Linux (2 Go de mémoire suffisent) avec Docker Engine et le
  module Docker Compose.
- Les ports 80 et 443 ouverts.
- Deux adresses qui pointent vers l'IP du serveur (enregistrements DNS `A`,
  et `AAAA` si le serveur a une IPv6) :
  - l'**administration**, par exemple `cms.exemple.fr` ;
  - le **site principal**, par exemple `www.exemple.fr`.

  Une seule adresse est possible : le site et son administration (`/admin`)
  partagent alors le même nom.

## Installer

```bash
git clone https://github.com/GZRC/awningcms.git
cd awningcms
sudo ./install.sh
```

Le script :

1. vérifie Docker et Docker Compose ;
2. demande les deux adresses et l'IP publique du serveur (proposée d'après sa
   route réseau) ;
3. crée dans `private/` le mot de passe de PostgreSQL, celui de l'application
   et le code de configuration. Ces fichiers ne sont jamais remplacés ;
4. écrit les réglages dans `infra/.env`, construit l'image et démarre Caddy,
   l'application et PostgreSQL.

Les réponses peuvent être données à l'avance, pour une installation
automatique : `AWNING_ADMIN_HOST`, `AWNING_SITE_HOST`, `AWNING_PUBLIC_IP`,
`AWNING_HTTP_PORT`, `AWNING_HTTPS_PORT`, `AWNING_PROJECT` et `AWNING_YES=1`.

## Première connexion

Ouvrez `https://<administration>/admin/setup`. Le code de configuration se lit
sur le serveur :

```bash
sudo cat private/setup-token.secret
```

Saisissez-le avec votre nom, votre e-mail, un mot de passe d'au moins
12 caractères et le nom du site. Le compte administrateur et le premier site
sont créés, et le code ne sert plus.

## Sites et domaines supplémentaires

Chaque site créé dans l'administration est visible sur
`https://<site principal>/s/<identifiant>`. Pour lui donner son propre
domaine, listez les domaines autorisés dans un fichier, par exemple
`private/domains.json` :

```json
{"source":"Mon registraire","synced_at":"2026-11-13T00:00:00Z","domains":["exemple-deux.fr"]}
```

puis déclarez-le dans `infra/compose.override.yaml`, que l'installateur et la
sauvegarde prennent en compte :

```yaml
services:
  app:
    environment:
      DOMAIN_CATALOG_FILE: /run/config/domains.json
    volumes:
      - ../private/domains.json:/run/config/domains.json:ro
```

Relancez `sudo ./install.sh`. La carte **Assistant IA · Domaines** de chaque site vérifie les
DNS puis connecte le domaine ; Caddy n'obtient son certificat qu'après cette
vérification. Avec un serveur [Ollama](https://ollama.com) joignable
(`OLLAMA_BASE_URL`, `OLLAMA_MODEL`), l'assistant peut aussi proposer un domaine
de la liste.

## Sauvegardes

`scripts/backup.sh` enregistre la base dans `/var/backups/awningcms`
(variable `BACKUP_DIR`) et garde 14 jours. Pour une sauvegarde chaque nuit :

```bash
sed "s#@INSTALL_DIR@#$PWD#" infra/systemd/awningcms-backup.service | sudo tee /etc/systemd/system/awningcms-backup.service
sudo cp infra/systemd/awningcms-backup.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now awningcms-backup.timer
```

Pour restaurer une sauvegarde, arrêtez l'application, rechargez la base, puis
redémarrez :

```bash
compose="docker compose --env-file infra/.env -f infra/compose.yaml"
sudo $compose stop app
sudo $compose exec -T postgres pg_restore -U cms -d cms --clean --if-exists < /var/backups/awningcms/cms-<date>.dump
sudo $compose start app
```

## Mettre à jour

```bash
git pull
sudo ./install.sh
```

Les migrations de la base s'appliquent au démarrage de l'application.
