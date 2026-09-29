# Quota Codex

[![Desktop checks](https://github.com/mathiasbunnens/chatgpt-quota-monitor/actions/workflows/check.yml/badge.svg)](https://github.com/mathiasbunnens/chatgpt-quota-monitor/actions/workflows/check.yml)
[![Latest release](https://img.shields.io/github/v/release/mathiasbunnens/chatgpt-quota-monitor)](https://github.com/mathiasbunnens/chatgpt-quota-monitor/releases/latest)
[![Platforms](https://img.shields.io/badge/plateformes-macOS%20%7C%20Windows%20%7C%20Linux-555)](docs/PLATFORMS.md)
[![Built with Tauri 2](https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri&logoColor=white)](src-tauri/Cargo.toml)
[![License: MIT](https://img.shields.io/badge/licence-MIT-blue.svg)](LICENSE)

**Les quotas Codex visibles en un coup d’œil, directement depuis la barre des menus ou la zone de notification.**

<img width="300" alt="Aperçu du menu macOS de Quota Codex" src="https://github.com/user-attachments/assets/5def5240-3c0b-42b2-ba33-d31215adbf63" />

Quota Codex est une petite application de bureau pour **macOS, Windows et Linux**. Elle se connecte localement à Codex, récupère les limites associées au compte ChatGPT et affiche le quota restant sans devoir garder une page web ouverte.

[Télécharger la dernière version](https://github.com/mathiasbunnens/chatgpt-quota-monitor/releases/latest) · [Guide d’installation](docs/INSTALLATION.md) · [Compatibilité](docs/PLATFORMS.md) · [Signaler un problème](https://github.com/mathiasbunnens/chatgpt-quota-monitor/issues/new/choose)

## Ce que l’application affiche

- le quota glissant de **5 heures** ;
- le quota **hebdomadaire** ;
- l’heure ou la date de réinitialisation de chaque limite ;
- une barre de progression verte, orange puis rouge à mesure que le quota diminue ;
- la réserve **Luna**, uniquement lorsqu’elle est réellement fournie par Codex et que le quota de 5 heures est épuisé.

Le quota hebdomadaire reste visible dès lors qu’il est fourni par Codex. Lorsque le quota de 5 heures atteint `0 %`, cette ligne disparaît et la réserve Luna prend sa place si elle est disponible. Quota Codex n’invente jamais une limite absente de la réponse de Codex.

## Fonctionnement

Quota Codex utilise directement **Codex App Server** en lecture seule :

1. l’application détecte une installation compatible de Codex ;
2. Codex réutilise la connexion ChatGPT déjà présente, ou propose une connexion si nécessaire ;
3. Quota Codex demande uniquement l’état du compte et les fenêtres de quota ;
4. les valeurs sont actualisées automatiquement en arrière-plan.

Il n’y a **aucune extension navigateur**, aucun cookie copié et aucun serveur HTTP local. La page d’utilisation Codex peut être ouverte pour consulter les détails, mais elle n’a pas besoin de rester ouverte.

## Installation rapide

Télécharge le paquet correspondant à ton système depuis la [dernière release GitHub](https://github.com/mathiasbunnens/chatgpt-quota-monitor/releases/latest).

| Système | Installation | Comportement |
| --- | --- | --- |
| macOS | Ouvre le `.dmg`, puis glisse **Quota Codex** dans **Applications**. | L’application reste uniquement dans la barre des menus. |
| Windows | Lance le fichier `*-setup.exe`. | Le tableau de bord et l’icône de notification restent disponibles. |
| Linux Debian/Ubuntu | Installe le `.deb` avec `sudo apt install ./nom-du-paquet.deb`. | Le tableau de bord fonctionne même sans zone de notification. |
| Linux AppImage | Rends l’AppImage exécutable, puis lance-le. | FUSE peut être nécessaire selon la distribution. |

Au premier lancement :

1. vérifie que Codex est installé ; sur macOS, Quota Codex détecte aussi le binaire intégré à l’application **ChatGPT** ;
2. ouvre **Connexion** si aucun compte ChatGPT n’est encore associé à Codex ;
3. termine l’autorisation dans le navigateur ;
4. reviens dans Quota Codex : les limites apparaissent automatiquement.

Une clé API seule ne donne pas accès aux quotas d’un abonnement ChatGPT. La connexion doit être effectuée avec le compte ChatGPT concerné.

> Le paquet macOS n’est pas signé avec un certificat Apple. Si macOS bloque le premier lancement, fais un clic droit sur **Quota Codex** dans Applications, choisis **Ouvrir**, puis confirme.

Le [guide d’installation détaillé](docs/INSTALLATION.md) couvre aussi les chemins personnalisés, le code de connexion et les problèmes fréquents.

## Une interface adaptée à chaque système

### macOS

Quota Codex est une application **menu-bar only** : aucune fenêtre principale ne s’ouvre. Un clic sur l’icône affiche les quotas et déclenche immédiatement leur actualisation. L’entrée **Connexion** disparaît dès que le compte est prêt.

### Windows

Le tableau de bord donne accès aux quotas, à la connexion et aux réglages d’actualisation. Fermer la fenêtre la masque lorsque l’icône de notification est disponible ; l’action **Quitter** arrête réellement l’application.

### Linux

Le tableau de bord reste la référence, car toutes les interfaces de bureau ne proposent pas une zone de notification compatible. Pour conserver le suivi actif, minimise la fenêtre au lieu de la fermer.

## Actualisation intelligente

L’application adapte sa fréquence d’actualisation au plan et au nombre estimé d’instances Codex ouvertes :

- au repos : toutes les 5 minutes ;
- Plus, Pro, Team, Business, Enterprise ou Edu : 60 secondes avec une instance, 30 secondes avec 2–3, puis 15 secondes avec 4 ou plus ;
- Free ou plan inconnu : 120, 60 puis 30 secondes selon l’activité.

Windows et Linux permettent aussi de choisir un intervalle fixe entre 30 secondes et 10 minutes. En cas d’erreur, les anciennes valeurs sont retirées et une nouvelle tentative est planifiée avec temporisation.

## Mises à jour

Quota Codex vérifie les nouvelles versions publiées sur GitHub. Lorsqu’une mise à jour est disponible, une seule popup accompagne tout le parcours :

1. recherche de la nouvelle version ;
2. téléchargement avec progression ;
3. installation ;
4. bouton **Quitter et relancer**.

Le bouton **Fermer** masque uniquement cette popup et laisse Quota Codex actif. Les artefacts de mise à jour sont signés et vérifiés par l’application.

## Confidentialité et sécurité

- aucune conversation et aucune tâche d’agent ne sont créées ;
- aucun mot de passe, cookie ou jeton ChatGPT n’est exposé au frontend ;
- Codex conserve et renouvelle lui-même sa session ;
- aucun quota n’est envoyé vers un serveur géré par ce projet ;
- la détection d’activité s’appuie uniquement sur les métadonnées locales des processus du même utilisateur ;
- le code source et la configuration de publication sont disponibles dans ce dépôt.

Voir également le [rapport de sécurité](docs/SECURITY_REVIEW.md) et la [politique de sécurité](SECURITY.md).

## Dépannage rapide

**Codex est introuvable**  
Installe Codex CLI ou indique le chemin complet de son exécutable. Sur macOS, l’application ChatGPT officielle est également détectée. Sous Windows, sélectionne le véritable fichier `codex.exe`, pas un script `.cmd`.

**Le bouton Connexion reste affiché**  
Vérifie que Codex utilise bien une connexion ChatGPT. Une authentification par clé API ne fournit pas les quotas de l’abonnement.

**Les quotas ne s’affichent pas**  
Vérifie la connexion Internet, mets Codex à jour, puis rouvre le menu ou relance l’application. Les limites non renvoyées par Codex ne sont pas fabriquées.

**L’icône Linux n’apparaît pas**  
Certaines interfaces de bureau n’hébergent pas les AppIndicators. Le tableau de bord continue de fonctionner normalement.

Pour aller plus loin, consulte [INSTALLATION.md](docs/INSTALLATION.md) et [PLATFORMS.md](docs/PLATFORMS.md).

## Développement

Prérequis : Node.js 22, npm, Rust stable et les [dépendances Tauri 2](https://v2.tauri.app/start/prerequisites/).

```sh
npm ci
npm run tauri dev
```

Vérifications locales :

```sh
npm run typecheck
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

Build de production sur le système courant :

```sh
npm run tauri build
```

Build macOS universel :

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri build -- --target universal-apple-darwin
```

Les builds de release sont générés sur macOS, Windows et Linux par [le workflow dédié](.github/workflows/release.yml). Les artefacts destinés à l’updater nécessitent la clé de signature du projet.

## Architecture

- `src-tauri/src/codex.rs` : découverte de Codex, connexion et lecture du protocole App Server ;
- `src-tauri/src/lib.rs` : sélection des quotas, menus natifs, fenêtres et mises à jour ;
- `src-tauri/src/activity.rs` : estimation locale du nombre d’instances Codex ;
- `src/` : tableau de bord Windows/Linux et popup de mise à jour ;
- `.github/workflows/` : vérifications et publication multiplateforme.

Les anciennes versions `1.x` utilisaient une extension Chromium. Elle n’est plus nécessaire et peut être supprimée du navigateur.

## Contribuer

Les contributions sont bienvenues. Consulte [CONTRIBUTING.md](CONTRIBUTING.md) avant d’ouvrir une pull request.

## Licence

[MIT](LICENSE)
