# AwningCMS

**Français** · [English](#english)

AwningCMS est un CMS libre pour les sites de petites entreprises : boutique,
restaurant, hôtel, salon, domaine viticole. Chaque entreprise a sa propre
installation, sur son serveur ou chez l'hébergeur de son choix.

## Ce que fait AwningCMS

- **Administration** protégée par session, créée une seule fois avec un code de
  configuration privé.
- **Plusieurs sites par installation**, chacun avec ses réglages, son tableau
  de bord et son adresse publique (`/s/{site}` ou son propre domaine).
- **Pages et articles** avec brouillon et version publiée distincts : modifier
  ne publie jamais automatiquement. Module Blog activable par site.
- **Éditeur visuel** des pages, de l'en-tête et du pied de page partagés, avec
  aperçu, révisions et publication atomique.
- **Extranet** pour l'équipe de chaque site : rôles Administrateur, Éditeur et
  Lecture seule, invitations, réinitialisation des mots de passe.
- **Comptes visiteurs** sur `/mon-compte`, sans accès à l'administration.
- **HTTPS automatique** avec Caddy, y compris pour les domaines des sites
  supplémentaires une fois vérifiés.
- **Sauvegarde quotidienne** de la base PostgreSQL.

L'interface est en français.

## Installation

Sur un serveur Linux avec [Docker Engine et Docker Compose](https://docs.docker.com/engine/install/),
après avoir fait pointer vos deux adresses (administration et site) vers le serveur :

```bash
git clone https://github.com/GZRC/awningcms.git
cd awningcms
sudo ./install.sh
```

Le script pose trois questions, crée les mots de passe dans `private/`, puis
démarre le CMS. Ouvrez ensuite l'adresse `/admin/setup` qu'il affiche.
Le guide complet est dans [docs/installation.md](docs/installation.md).

Pour mettre à jour : `git pull`, puis relancez `sudo ./install.sh`. Les
secrets et les données sont conservés.

## Développement

- Tests Rust : `cargo test`, `cargo clippy --all-targets -- -D warnings`.
- Test de bout en bout sur une pile jetable : `scripts/test-local.sh`.
- Tests de l'installateur : `sudo python3 -m unittest tests/test_install.py`.

## Licence

Copyright (C) 2026 Guillaume Cassagneres.

AwningCMS est un logiciel libre distribué sous la licence
[GNU Affero General Public License](LICENSE), version 3 ou toute version
ultérieure. Si vous proposez une version modifiée d'AwningCMS à des
utilisateurs sur un réseau, vous devez leur donner accès à son code source.

---

## English

AwningCMS is a free, self-hosted CMS for small business websites: shops,
restaurants, hotels, salons, wineries. Each business gets its own
installation, on its own server or with the host of its choice.

**Features:** owner administration created once with a private setup code;
several sites per installation; pages and articles with separate draft and
published versions; a visual page, header and footer builder; a per-site
extranet with Administrator, Editor and Read-only roles; visitor accounts;
automatic HTTPS with Caddy, including verified custom domains; daily
PostgreSQL backups. The interface is in French.

**Install** on a Linux server with Docker Engine and Docker Compose, once your
administration and site addresses point to the server:

```bash
git clone https://github.com/GZRC/awningcms.git
cd awningcms
sudo ./install.sh
```

Then open the `/admin/setup` address the script prints. See
[docs/installation.md](docs/installation.md) (French). To update, `git pull`
and run `sudo ./install.sh` again; secrets and data are kept.

**License:** Copyright (C) 2026 Guillaume Cassagneres. AwningCMS is free
software under the [GNU Affero General Public License](LICENSE), version 3 or
any later version.
