# Quota Monitor

**Vos quotas Codex et Claude, en un coup d’œil, depuis la barre des menus.**

Une petite application de bureau pour macOS, Windows et Linux qui lit localement les limites de vos abonnements ChatGPT (Codex) et Claude (Claude Code), sans page web ouverte ni extension.

## Aperçu

| menu | réglages |
| :---: | :---: |
| <img width="400"  alt="Menu de Quota Monitor : cartes Codex et Claude" src="https://github.com/user-attachments/assets/fa1e26c0-3dc2-43d8-a097-c3f6a287d056" /> | <img width="400"  alt="Réglages de Quota Monitor : fournisseurs Codex et Claude" src="https://github.com/user-attachments/assets/c0fc93ef-49d1-42a3-bc49-21da7f74bab7" /> |

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
### Notifications téléphone (optionnelles)

Dans **Réglages → Notifications**, renseignez l’URL du serveur (`https://ntfy.sh`
par défaut, ou votre serveur ntfy) et un topic long, aléatoire et imprévisible.
Installez ntfy sur le téléphone, abonnez-vous au même topic sur le même serveur et
autorisez les notifications. **Envoyer un test** utilise les champs sans les
enregistrer et indique si le serveur a accepté le POST, pas si le téléphone l’a reçu.
Cochez les alertes automatiques puis **Enregistrer** ; elles sont désactivées par défaut.
Sans authentification, toute personne connaissant le topic peut lire et publier :
ne partagez pas ce nom. Aucun jeton ntfy n’est pris en charge ; préférez HTTPS.

L’application doit rester ouverte et connectée. Seuls le fournisseur, la fenêtre,
le seuil atteint et la date de réinitialisation connue sont transmis (aucun compte,
abonnement ou secret). Les données inconnues, en erreur, expirées ou de plus de
deux minutes sont ignorées. Une première observation déjà épuisée déclenche aussi
une alerte. Une seule tentative par fournisseur/fenêtre/cycle est enregistrée dans
`ntfy-attempts.json` avant l’envoi, y compris si le réseau échoue : pas de spam ni
de répétition après redémarrage, mais une interruption peut perdre une alerte.
Un retour sous 100 % consommé ou une nouvelle date de reset réarme la fenêtre.
Changer de destination ou désactiver/réactiver ne rejoue pas les alertes déjà tentées.
Les réglages sont conservés dans `prefs.json` dans le dossier local de l’application.

### Confidentialité

- Aucune conversation ni requête de modèle n’est créée ; Codex et Claude Code gèrent eux-mêmes leur connexion.
- Aucun mot de passe, cookie ou jeton n’est lu ; les quotas et l’abonnement restent en mémoire, hormis l’état minimal de dédoublonnage des notifications optionnelles.
- Aucun quota n’est envoyé vers un serveur de ce projet.

## Pour aller plus loin

[Guide d’installation](docs/INSTALLATION.md) · [Plateformes](docs/PLATFORMS.md) · [Fournisseur Claude](docs/claude-provider.md) · [Sécurité](SECURITY.md)

Développement : `npm ci && npm run tauri dev`. Tests : `cargo test --manifest-path src-tauri/Cargo.toml`. Rendu hors écran du menu macOS : `chatgpt-quota-monitor --quota-ui-smoke <dossier>`.
