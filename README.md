# Quota Monitor

**Vos quotas Codex et Claude, en un coup d’œil, depuis la barre des menus.**

Une petite application de bureau pour macOS, Windows et Linux qui lit localement les limites de vos abonnements ChatGPT (Codex) et Claude (Claude Code), sans page web ouverte ni extension.

## Aperçu

| menu | réglages |
| :---: | :---: |
| <img width="300" alt="Menu de Quota Monitor : cartes Codex et Claude" src="docs/images/menu-macos.png" /> | <img width="400" alt="Réglages de Quota Monitor : fournisseurs Codex et Claude" src="docs/images/settings-macos.png" /> |

*Captures des vraies vues de l’application, avec des données de démonstration.*

## Ce que fait l’application

- Affiche pour Codex et Claude la limite 5 heures puis la limite hebdomadaire : pourcentage restant, temps avant réinitialisation (« Limite sur 1h38 », « Limite sur 5j 12h ») et heure de réinitialisation.
- Indique l’abonnement (Plus, Pro…) et les erreurs de lecture, fournisseur par fournisseur.
- Montre le quota 5 heures dans la barre des menus : Codex, Claude, ou les deux l’un au-dessus de l’autre.
- Se met à jour en arrière-plan et à chaque ouverture du menu, même lorsqu’il reste ouvert.
- Utilise une réinitialisation Codex en un clic, après confirmation.

## Installation

Téléchargez la [dernière version](https://github.com/mathiasbunnens/quota-monitor/releases/latest) :

- **macOS** : ouvrez le `.dmg` et glissez **Quota Monitor** dans Applications. L’application vit dans la barre des menus.
- **Windows** : lancez `*-setup.exe` (tableau de bord et icône de notification).
- **Linux** : `.deb` ou AppImage (tableau de bord).

Le paquet macOS n’est pas notarisé : au premier lancement, clic droit sur l’application → **Ouvrir**.

## Démarrer en 3 étapes

1. **Codex** : installez Codex CLI (ou l’app ChatGPT sur macOS), puis **Se connecter à Codex…** dans le menu si besoin.
2. **Claude** : installez Claude Code, lancez `claude auth login`, puis activez Claude dans **Réglages → Fournisseurs**.
3. Dans **Réglages → Barre des menus**, choisissez Codex, Claude ou les deux.

## Vos comptes restent protégés

- Aucune conversation ni requête de modèle n’est créée ; Codex et Claude Code gèrent eux-mêmes leur connexion.
- Aucun mot de passe, cookie ou jeton n’est lu ; seuls les pourcentages, réinitialisations et l’abonnement sont conservés, en mémoire.
- Aucun quota n’est envoyé vers un serveur de ce projet.

## Pour aller plus loin

[Guide d’installation](docs/INSTALLATION.md) · [Plateformes](docs/PLATFORMS.md) · [Fournisseur Claude](docs/claude-provider.md) · [Sécurité](SECURITY.md)

Développement : `npm ci && npm run tauri dev`. Tests : `cargo test --manifest-path src-tauri/Cargo.toml`. Rendu hors écran du menu macOS : `chatgpt-quota-monitor --quota-ui-smoke <dossier>`.
