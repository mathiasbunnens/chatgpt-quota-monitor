# Quota Codex

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Latest release](https://img.shields.io/github/v/release/mathiasbunnens/chatgpt-quota-monitor)](https://github.com/mathiasbunnens/chatgpt-quota-monitor/releases/latest)
[![Built with Tauri 2](https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri&logoColor=white)](src-tauri/Cargo.toml)
[![Multiplatform CI](https://github.com/mathiasbunnens/chatgpt-quota-monitor/actions/workflows/check.yml/badge.svg?branch=multi_platform_support)](https://github.com/mathiasbunnens/chatgpt-quota-monitor/actions/workflows/check.yml?query=branch%3Amulti_platform_support)

> **Windows, Linux et macOS :** la version multiplateforme est développée dans la branche [multi_platform_support](https://github.com/mathiasbunnens/chatgpt-quota-monitor/tree/multi_platform_support). Elle propose une connexion directe à Codex, un pourcentage dans la zone de notification et une actualisation dynamique. Le badge « Multiplatform CI » concerne cette branche. Les instructions macOS ci-dessous décrivent la version de cette branche principale.

[Guide multiplateforme](https://github.com/mathiasbunnens/chatgpt-quota-monitor/blob/multi_platform_support/docs/INSTALLATION.md) · [Contribuer](https://github.com/mathiasbunnens/chatgpt-quota-monitor/blob/multi_platform_support/CONTRIBUTING.md) · [Revue de sécurité](https://github.com/mathiasbunnens/chatgpt-quota-monitor/blob/multi_platform_support/docs/SECURITY_REVIEW.md)

Widget macOS natif qui affiche le quota Codex restant directement dans la barre des menus pour un abonnement ChatGPT Plus.

<img width="300" alt="Capture d’écran 2026-09-23 à 23 36 38 2" src="https://github.com/user-attachments/assets/5def5240-3c0b-42b2-ba33-d31215adbf63" />

Un clic sur la mascotte ouvre un menu compact avec :

- la limite glissante de 5 heures ;
- la limite globale hebdomadaire ;
- l’heure de réinitialisation de chaque limite ;
- une barre de progression verte, orange puis rouge à mesure que le quota diminue.

L’application ne lit ni mot de passe, ni cookie, ni token. Une petite extension Chromium relève uniquement les informations déjà visibles sur la page d’utilisation Codex et les transmet localement à l’application via `127.0.0.1`.

## Installation rapide

1. Télécharge **Quota Codex** depuis la [dernière release GitHub](../../releases/latest).
2. Ouvre le fichier `.dmg`, puis glisse **Quota Codex** dans **Applications**.
3. Décompresse l’archive de l’extension dans un dossier que tu conserveras.
4. Dans Chrome, Brave, Edge ou Arc, ouvre la page des extensions, active le **Mode développeur**, puis choisis **Charger l’extension non empaquetée** et sélectionne le dossier `extension` décompressé.
5. Lance **Quota Codex**, connecte-toi à ChatGPT, puis ouvre <https://chatgpt.com/codex/settings/usage>.

Les valeurs apparaissent ensuite dans la barre des menus et sont actualisées automatiquement.

## Mises à jour

Quota Codex vérifie les nouvelles releases GitHub au démarrage. Les mises à jour sont signées, téléchargées puis installées automatiquement avant le redémarrage de l’application.

> La première version distribuée n’est pas signée avec un certificat Apple. Si macOS bloque son lancement, fais un clic droit sur **Quota Codex** dans Applications, choisis **Ouvrir**, puis confirme une seconde fois.

Le guide détaillé, avec les adresses propres à chaque navigateur et les solutions aux problèmes courants, se trouve dans [docs/INSTALLATION.md](docs/INSTALLATION.md).

## Confidentialité

- aucune donnée n’est envoyée vers un serveur tiers par ce projet ;
- l’extension ne demande aucune permission de lecture des cookies ou du stockage du navigateur ;
- le transfert navigateur → application reste sur la machine, via `http://127.0.0.1:48721/quota` ;
- le code source de l’application et de l’extension est intégralement disponible dans ce dépôt.

## Compatibilité

- macOS, Apple Silicon et Intel ;
- Chrome, Brave, Edge, Arc et autres navigateurs Chromium compatibles Manifest V3 ;
- une session ChatGPT connectée avec accès à la page d’utilisation Codex.

Safari et Firefox ne sont pas encore pris en charge.

## Développement

Prérequis : Node.js, npm, Rust et les dépendances système de Tauri 2.

```bash
npm install
npm run tauri dev
```

Vérifications :

```bash
npm run typecheck
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

Build macOS local :

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri build -- --target universal-apple-darwin
```

Les paquets sont générés dans `src-tauri/target/universal-apple-darwin/release/bundle/`.

## Architecture

- `src-tauri/` : widget AppKit/Tauri, menu natif et bridge HTTP local ;
- `extension/` : extension Chromium Manifest V3 qui lit la page d’utilisation ;
- `src/` : ancien frontend de développement, conservé pour les composants et les types ;
- `.github/workflows/release.yml` : génération automatique des artefacts lors de la publication d’un tag `v*`.

## Licence

[MIT](LICENSE)
