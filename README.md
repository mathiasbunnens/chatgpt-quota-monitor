# Quota Codex

Application de suivi des quotas Codex pour **Windows, Linux et macOS**. Codex App Server est la source principale : aucune extension ni page ouverte n’est nécessaire après connexion.

- Détection de Codex et réutilisation de sa connexion ChatGPT.
- Connexion depuis l’application, avec option de code pour un autre appareil.
- Affichage de toutes les fenêtres renvoyées, même si seule une limite hebdomadaire est disponible.
- Actualisation toutes les 60 secondes et reconnexion avec temporisation après erreur.
- Bouton **Voir les détails sur Codex** pour ouvrir la page d’utilisation.
- Menu natif macOS ; tableau de bord et zone de notification sous Windows/Linux.
- Extension Chromium facultative comme source de secours.

## Installation

Installe Quota Codex, puis lance-le. Si Codex est déjà installé et connecté à ChatGPT, les quotas apparaissent automatiquement. Sinon, ouvre **Connexion** et suis les instructions.

Un [Codex CLI](https://developers.openai.com/codex/cli) compatible doit être installé séparément ; il n’est pas inclus dans le paquet. Les chemins usuels et le PATH sont recherchés. Un chemin personnalisé peut être enregistré ; sous Windows, choisis le véritable fichier `codex.exe`, pas un script `.cmd`.

Choisis **Se connecter avec ChatGPT** ou **Connexion par code** si nécessaire. Termine l’autorisation, puis ferme le navigateur. Voir [INSTALLATION.md](docs/INSTALLATION.md) et [PLATFORMS.md](docs/PLATFORMS.md).

Les paquets de cette branche seront disponibles sur GitHub après une release construite avec son workflow. Les anciennes versions peuvent encore nécessiter l’extension.

## Données et connexion

Codex gère les identifiants et le renouvellement des jetons. Quota Codex demande uniquement l’état du compte et les quotas, sans conversation ni tâche d’agent. L’interface ne reçoit aucun jeton du compte. Une connexion ChatGPT est nécessaire ; une clé API seule ne fournit pas les quotas d’abonnement.

L’extension de secours lit uniquement les valeurs visibles et les transmet à `127.0.0.1:48721`. Elle ne lit ni cookies ni mots de passe. Elle ne remplace jamais une réponse Codex valide. Les limites absentes ne sont pas interprétées comme illimitées.

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
- `src-tauri/src/lib.rs` : priorité des sources, menus, fenêtres et bridge de secours.
- `src-tauri/src/setup.rs` : navigateur, page de détails et extension facultative.
- `src/` : tableau de bord et connexion sur les trois plateformes.
- `.github/workflows/` : vérifications et paquets multiplateformes.

Les mises à jour pointent encore vers le dépôt d’origine. Configure une URL et une clé de signature propres avant de distribuer un fork.

## Licence

[MIT](LICENSE)
