#!/usr/bin/env bash
# Exécuté sur le VPS par GitHub Actions, dans /opt/volta, après l'envoi des fichiers.
set -euo pipefail
cd /opt/volta
SHA="$1"

mkdir -p tesla
touch .env
chmod 600 .env
# S'assure que .env finit par un retour à la ligne avant d'y ajouter quoi que ce soit.
[ -z "$(tail -c1 .env)" ] || echo >> .env

# Clé de chiffrement des jetons Tesla : générée ici une seule fois, ne quitte jamais le serveur.
if ! grep -q '^TOKEN_ENCRYPTION_KEY=' .env; then
  echo "TOKEN_ENCRYPTION_KEY=$(openssl rand -base64 32)" >> .env
fi

# Identifiants Tesla venant des secrets GitHub : remplacent ceux déjà présents.
if [ -s tesla.env ]; then
  grep -v -e '^TESLA_CLIENT_ID=' -e '^TESLA_CLIENT_SECRET=' .env > .env.new || true
  cat tesla.env >> .env.new
  chmod 600 .env.new
  mv .env.new .env
fi
rm -f tesla.env

gunzip -c images.tar.gz | docker load
rm images.tar.gz
VOLTA_VERSION="$SHA" docker compose up -d --remove-orphans
docker image prune -f
