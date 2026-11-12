#!/usr/bin/env bash
# Installation initiale du VPS (Ubuntu 24.04). À lancer UNE fois, en root :
#   curl -fsSL https://raw.githubusercontent.com/viorizz/volta/main/deploy/setup-vps.sh -o setup-vps.sh
#   less setup-vps.sh        # relis-le avant de l'exécuter
#   sudo bash setup-vps.sh
set -euo pipefail

[[ $EUID -eq 0 ]] || { echo "À lancer en root : sudo bash setup-vps.sh"; exit 1; }

read -rp "Nom de domaine du site (ex. volta.mondomaine.ch) : " DOMAIN
[[ -n "$DOMAIN" ]] || { echo "Domaine requis"; exit 1; }

echo "==> Paquets : Docker, pare-feu, mises à jour de sécurité automatiques"
apt-get update -q
DEBIAN_FRONTEND=noninteractive apt-get install -yq docker.io docker-compose-v2 ufw argon2 unattended-upgrades openssl
systemctl enable --now docker

echo "==> Pare-feu : SSH, HTTP, HTTPS uniquement"
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw allow 443/udp
ufw --force enable

echo "==> Utilisateur 'deploy' (utilisé par GitHub Actions, sans sudo)"
id deploy &>/dev/null || useradd --create-home --shell /bin/bash deploy
usermod -aG docker deploy
install -d -o deploy -g deploy -m 750 /opt/volta /opt/volta/backups

if [[ ! -f /opt/volta/.env ]]; then
  read -rsp "Mot de passe pour se connecter au site (12 caractères min.) : " PW; echo
  [[ ${#PW} -ge 12 ]] || { echo "Trop court"; exit 1; }
  HASH=$(printf '%s' "$PW" | argon2 "$(openssl rand -hex 16)" -id -t 3 -m 16 -p 1 -e)
  unset PW
  # Guillemets simples : le hash contient des '$' que Docker Compose ne doit pas interpréter.
  cat > /opt/volta/.env <<ENV
DOMAIN=$DOMAIN
POSTGRES_PASSWORD=$(openssl rand -hex 24)
ADMIN_PASSWORD_HASH='$HASH'
ENV
  chown deploy:deploy /opt/volta/.env
  chmod 600 /opt/volta/.env
  echo "    /opt/volta/.env créé"
else
  echo "    /opt/volta/.env existe déjà, conservé"
fi

echo "==> Sauvegarde quotidienne de la base (03:15, 30 jours gardés)"
cat > /etc/cron.d/volta-backup <<'CRON'
15 3 * * * deploy cd /opt/volta && docker compose exec -T db pg_dump -U volta volta | gzip > backups/volta-$(date +\%F).sql.gz && find backups -name '*.sql.gz' -mtime +30 -delete
CRON

echo "==> Clé SSH dédiée au déploiement"
install -d -o deploy -g deploy -m 700 /home/deploy/.ssh
TMP=$(mktemp -d)
ssh-keygen -t ed25519 -N "" -C "github-actions-volta" -f "$TMP/key" -q
# 'restrict' : pas de redirection de ports ni de terminal interactif pour cette clé.
echo "restrict $(cat "$TMP/key.pub")" >> /home/deploy/.ssh/authorized_keys
chown deploy:deploy /home/deploy/.ssh/authorized_keys
chmod 600 /home/deploy/.ssh/authorized_keys

IP=$(hostname -I | awk '{print $1}')
cat <<MSG

======================================================================
 Installation terminée. Dans GitHub : viorizz/volta > Settings >
 Secrets and variables > Actions, ajoute :

 Onglet "Variables" :
   VPS_HOST         = $IP

 Onglet "Secrets" :
   VPS_KNOWN_HOSTS  = la ligne suivante :
$(for k in /etc/ssh/ssh_host_ed25519_key.pub; do echo "$IP $(cut -d' ' -f1,2 "$k")"; done)

   VPS_SSH_KEY      = tout le bloc suivant (BEGIN et END compris) :
$(cat "$TMP/key")
======================================================================
 Cette clé privée n'est affichée qu'une fois et va être supprimée du
 serveur. Vérifie aussi que le DNS de $DOMAIN pointe vers $IP.
======================================================================
MSG
rm -rf "$TMP"
