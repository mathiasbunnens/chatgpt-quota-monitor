# Quota Codex

[![Desktop checks](https://github.com/mathiasbunnens/chatgpt-quota-monitor/actions/workflows/check.yml/badge.svg?branch=multi_platform_support)](https://github.com/mathiasbunnens/chatgpt-quota-monitor/actions/workflows/check.yml?query=branch%3Amulti_platform_support)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux%20%7C%20macOS-555)](docs/PLATFORMS.md)
[![Built with Tauri 2](https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri&logoColor=white)](src-tauri/Cargo.toml)

**A desktop Codex quota monitor with tray percentages, direct account sync, and activity-aware refresh.**

[Installation](docs/INSTALLATION.md) · [Plateformes](docs/PLATFORMS.md) · [Contribuer](CONTRIBUTING.md) · [Sécurité](SECURITY.md) · [Signaler un problème](https://github.com/mathiasbunnens/chatgpt-quota-monitor/issues/new/choose)

> Cette branche (`multi_platform_support`) contient la version multiplateforme en développement. Le badge CI concerne cette branche ; les anciennes releases et `main` peuvent différer.

Application de suivi des quotas Codex pour **Windows, Linux et macOS**. Codex App Server est la seule source : aucune extension ni page ouverte n’est nécessaire après connexion.

- Détection de Codex et réutilisation de sa connexion ChatGPT.
- Connexion depuis l’application, avec option de code pour un autre appareil.
- Affichage de toutes les fenêtres renvoyées, même si seule une limite hebdomadaire est disponible.
- Actualisation dynamique selon le plan et le nombre estimé d’instances Codex : au repos 5 min ; pour Plus/Pro/équipe, 60 s (1), 30 s (2–3), 15 s (4+). Free/inconnu : 120/60/30 s. Choix propres à cette application.
- Panneau latéral Réglages : mode dynamique ou intervalle personnalisé (30 s à 10 min), conservé au redémarrage. Les erreurs entraînent une temporisation.
- Bouton **Voir les détails sur Codex** pour ouvrir la page d’utilisation.
- Interface commune inspirée de macOS, modes clair/sombre et mêmes actions dans les menus Windows, Linux et macOS. Les menus natifs conservent le rendu du système.
- Pour Plus, la fenêtre de 5 heures est prioritaire et masque la réserve. À 0 %, elle disparaît au profit de la réserve lorsqu’elle est publiée par Codex. Les comptes sans fenêtre de 5 heures affichent les fenêtres disponibles ; une limite hebdomadaire n’est jamais renommée « réserve ».
- Pourcentage dans la barre macOS et badge numérique dans la zone de notification Windows/Linux ; le survol précise la limite suivie. La disponibilité de la zone de notification Linux dépend de l’environnement de bureau.

## Installation

Installe Quota Codex, puis lance-le. Si Codex est déjà installé et connecté à ChatGPT, les quotas apparaissent automatiquement. Sinon, ouvre **Connexion** et suis les instructions.

Un [Codex CLI](https://developers.openai.com/codex/cli) compatible doit être installé séparément ; il n’est pas inclus dans le paquet. Les chemins usuels et le PATH sont recherchés. Un chemin personnalisé peut être enregistré ; sous Windows, choisis le véritable fichier `codex.exe`, pas un script `.cmd`.

Choisis **Se connecter avec ChatGPT** ou **Connexion par code** si nécessaire. Termine l’autorisation, puis ferme le navigateur. Voir [INSTALLATION.md](docs/INSTALLATION.md) et [PLATFORMS.md](docs/PLATFORMS.md).

Les paquets de cette branche seront disponibles sur GitHub après une release construite avec son workflow. Les anciennes versions peuvent encore nécessiter l’extension.

## Données et connexion

Codex gère les identifiants et le renouvellement des jetons. Quota Codex demande uniquement l’état du compte et les quotas, sans conversation ni tâche d’agent. L’interface ne reçoit aucun jeton du compte. Une connexion ChatGPT est nécessaire ; une clé API seule ne fournit pas les quotas d’abonnement.

Aucun serveur HTTP local ni collecte par extension. Les limites absentes ne sont pas interprétées comme illimitées.

## Développement

Prérequis : Node.js 22, npm, Rust et les [dépendances Tauri](https://v2.tauri.app/start/prerequisites/).

```sh
npm ci
npm run tauri dev
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

Test facultatif en lecture seule avec une connexion Codex existante :

```sh
cargo test --manifest-path src-tauri/Cargo.toml --lib live_codex_account_read -- --ignored --nocapture
```

## Architecture

- `src-tauri/src/codex.rs` : processus Codex, connexion et protocole stdio.
- `src-tauri/src/lib.rs` : quotas, menus, fenêtres et lien de détails.
- `src-tauri/src/activity.rs` : estimation locale des instances par métadonnées de processus.
- `src/` : tableau de bord et connexion sur les trois plateformes.
- `.github/workflows/` : vérifications et paquets multiplateformes.

Les mises à jour pointent encore vers le dépôt d’origine. Configure une URL et une clé de signature propres avant de distribuer un fork.

## Licence

[MIT](LICENSE)

## Sécurité

Voir [le rapport de revue](docs/SECURITY_REVIEW.md) pour le périmètre, les correctifs et les limites de vérification. Les anciennes extensions installées peuvent être supprimées manuellement ; elles ne sont plus utilisées.
