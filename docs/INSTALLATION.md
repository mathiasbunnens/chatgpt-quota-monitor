# Installer Quota Codex

## Installer l’application

- Windows : lance `*-setup.exe`, puis Quota Codex depuis la page de fin ou le menu Démarrer.
- Linux Debian : `sudo apt install ./nom-du-paquet.deb`.
- Linux AppImage : rends le fichier exécutable puis lance-le ; FUSE peut être nécessaire.
- macOS : copie l’application du DMG dans Applications. Fermer la fenêtre conserve le menu natif.

## Connexion directe à Codex

Codex CLI doit être installé séparément. Une installation compatible déjà connectée à ChatGPT suffit. Sinon, clique sur **Connexion → Installer Codex**, puis **Détection automatique** après son installation. Un chemin complet peut être défini dans **Emplacement de Codex**. Sous Windows, choisis `codex.exe`.

**Se connecter avec ChatGPT** ouvre l’autorisation dans le navigateur. **Connexion par code** permet de la terminer sur un autre appareil si cette méthode est activée dans les paramètres du compte ou de l’espace de travail. Après connexion, le navigateur peut être fermé. Codex conserve la session.

**Voir les détails sur Codex** ouvre la page d’utilisation à tout moment. Pour comparer les chiffres, utilise le même compte dans le navigateur et dans Codex.

Les quotas sont actualisés selon le plan et les instances Codex détectées. Le panneau Réglages permet de choisir le mode dynamique ou une fréquence fixe. Une erreur retire les anciennes données ; aucun secours navigateur ne collecte de quotas.

## Dépannage

- Codex introuvable : vérifie le chemin et les permissions. Sous Windows, les scripts npm `.cmd` ne sont pas utilisés ; indique le binaire natif.
- Compte non connecté : utilise ChatGPT ; une clé API seule ne fournit pas les quotas de l’abonnement.
- Lecture impossible : vérifie Internet et la version de Codex, puis réessaie.
- Linux sans zone de notification : utilise la fenêtre. Fermer quitte ; minimiser conserve le suivi.
- Windows : fermer masque la fenêtre si le tray est disponible ; **Quitter** termine l’application.

## Désinstallation

Quitte Quota Codex et désinstalle son paquet. Supprime aussi l’extension du navigateur si tu l’as installée. Codex CLI et sa connexion partagée ne sont pas supprimés.
