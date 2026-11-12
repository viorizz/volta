# Volta

Tableau de bord personnel pour voiture électrique : historique de recharge, comparatif avec une voiture essence et suivi de toutes les dépenses (pneus, entretien, assurance…). Auto-hébergé sur un VPS.

L'interface suit le design system **Volta** : thème sombre, grands chiffres fins, boutons à coins de 4px, inspiré de l'app mobile du constructeur sans en reprendre la marque.

## Fonctionnalités

- **Recharges** : saisie manuelle (Supercharger, maison, borne publique, autre), import CSV avec choix des colonnes et sans doublons au réimport. Le coût d'une recharge à la maison est calculé à partir du tarif du kWh.
- **Dépenses** : pneus, entretien, assurance, taxes et vignette, accessoires, parking.
- **Aperçu** : totaux par mois, par année ou depuis le début, économie face à l'essence, coût aux 100 km, graphique mensuel électricité/essence (avec vue tableau), répartition par réseau et par catégorie.
- **Frais fixes et leasing** : assurance (RC, casco partielle ou complète), options du véhicule, abonnements de recharge, taxes et vignette ; leasing avec acompte, mensualité, taux, valeur résiduelle et km inclus (projection et coût des km en trop).
- **Coût de possession** : total, par mois et par km, dépréciation comprise (achat).
- **Entretien** : rappels par date et/ou kilométrage, récurrents (pneus, expertise, liquide de frein…), relevés du compteur, lu automatiquement sur la voiture si elle est éveillée.
- **Recharge à domicile estimée** (option) : l'énergie nécessaire d'après les km parcourus, moins les recharges saisies.
- **Budget mensuel** avec suivi sur l'aperçu.
- **Comparatif honnête** : la distance vient du compteur. Si le rapport km/kWh n'est pas plausible (recharges manquantes), l'app le signale au lieu d'afficher une économie fausse.

## Stack

| Couche | Choix |
| --- | --- |
| Backend | Rust, Axum 0.8, SQLx 0.8 (requêtes vérifiées à la compilation) |
| Base | PostgreSQL 16 |
| Frontend | React 18, TypeScript, Vite |
| Serveur web | Caddy (HTTPS automatique, en-têtes de sécurité) |
| Déploiement | Docker Compose, GitHub Actions → VPS par SSH |

Authentification : un seul utilisateur, mot de passe haché en Argon2id, session par cookie `HttpOnly; Secure; SameSite=Strict`, 5 essais puis blocage de 15 minutes.

## Développement local

Prérequis : Rust, Node 22, PostgreSQL.

```bash
# Base de données
createuser -P volta && createdb -O volta volta

# Backend
cd backend
export DATABASE_URL=postgres://volta:volta@localhost/volta
export ADMIN_PASSWORD_HASH=$(echo -n 'un-mot-de-passe-local' | cargo run -q -- hash-password)
COOKIE_SECURE=false BIND_ADDR=127.0.0.1:8080 cargo run

# Frontend (autre terminal) : http://localhost:5173
cd frontend && npm install && npm run dev
```

Après toute modification d'une requête SQL : `cargo sqlx prepare` dans `backend/` (avec `sqlx-cli`), puis commiter le dossier `.sqlx`. Sans ça, le build Docker échoue.

## Mise en production

1. Pointe un nom de domaine (enregistrement A) vers l'IP du VPS.
2. Sur le VPS (Ubuntu 24.04), en root :
   ```bash
   curl -fsSL https://raw.githubusercontent.com/viorizz/volta/main/deploy/setup-vps.sh -o setup-vps.sh
   less setup-vps.sh
   sudo bash setup-vps.sh
   ```
   Le script installe Docker et le pare-feu, crée l'utilisateur `deploy`, génère `/opt/volta/.env` (mot de passe de la base, hash du mot de passe du site), programme une sauvegarde quotidienne et affiche les valeurs à copier dans GitHub.
3. Dans GitHub, *Settings → Secrets and variables → Actions* : la variable `VPS_HOST` et les secrets `VPS_SSH_KEY` et `VPS_KNOWN_HOSTS`.
4. Relance le workflow *CI et déploiement* (onglet Actions). Chaque push sur `main` déploie ensuite automatiquement.

Les images sont compilées par GitHub Actions puis envoyées par SSH : rien ne compile sur le VPS, et aucun registre d'images n'est nécessaire.

Sauvegardes : `/opt/volta/backups/`, 30 jours. Pour restaurer : `gunzip -c backups/volta-AAAA-MM-JJ.sql.gz | docker compose exec -T db psql -U volta volta`.

## Connexion à l'API Tesla (Fleet API)

Importe automatiquement l'historique Supercharger (endpoint `charging_history`), toutes les 12 heures ou à la demande depuis les Réglages. Les autres réseaux restent à saisir à la main.

1. **Clé publique** : sur le VPS, en tant que `deploy` :
   ```bash
   mkdir -p /opt/volta/tesla && cd /opt/volta/tesla
   openssl ecparam -name prime256v1 -genkey -noout -out private-key.pem
   openssl ec -in private-key.pem -pubout -out com.tesla.3p.public-key.pem
   chmod 600 private-key.pem
   ```
   Caddy la sert sur `https://<domaine>/.well-known/appspecific/com.tesla.3p.public-key.pem`. La clé privée ne sert qu'à envoyer des commandes à la voiture, ce que Volta ne fait pas ; elle reste sur le serveur.
2. **Application** sur [developer.tesla.com](https://developer.tesla.com) (compte avec double authentification) :
   - Allowed Origin : `https://<domaine>`
   - Allowed Redirect URI : `https://<domaine>/api/tesla/callback`
   - Scopes : *Vehicle Information* et *Vehicle Charging Management*
   - Ajoute un moyen de paiement : sans lui, Tesla désactive l'application. Le crédit mensuel de 10 $ couvre largement un usage personnel.
3. **Identifiants** : dans GitHub, *Settings → Secrets and variables → Actions → Secrets*, ajoute `TESLA_CLIENT_ID` et `TESLA_CLIENT_SECRET`, puis relance le workflow. Le déploiement les écrit dans `/opt/volta/.env` (droits 600). Tu peux aussi les écrire toi-même dans ce fichier : sans secrets GitHub, le déploiement n'y touche pas.
4. **Enregistrement du domaine** (une seule fois) :
   ```bash
   cd /opt/volta && docker compose run --rm api volta-api tesla-register
   ```
5. Dans Volta : *Réglages → Connecter mon compte Tesla*.

Les jetons Tesla sont chiffrés en base (AES-256-GCM) avec `TOKEN_ENCRYPTION_KEY`, générée automatiquement dans `/opt/volta/.env` au premier déploiement. Les sauvegardes de la base ne contiennent donc pas de jeton lisible. Si la clé est perdue ou changée, il suffit de reconnecter le compte Tesla.

## À venir

- Export CSV.
