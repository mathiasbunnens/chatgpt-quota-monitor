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

Les quotas sont actualisés toutes les 60 secondes. Chaque ligne correspond à une fenêtre retournée ; une limite absente ne signifie pas illimitée. Une erreur de lecture retire les anciennes données directes et peut activer le secours si l’extension transmet des quotas.

## Extension facultative

Dans **Connexion → Solution de secours : extension navigateur** :

1. Choisis Chrome, Edge, Brave ou Chromium et ouvre sa page des extensions.
2. Active **Mode développeur**.
3. Clique sur **Charger l’extension non empaquetée** et sélectionne le dossier indiqué.
4. Connecte-toi à ChatGPT dans la page Codex ouverte à la première installation.

L’extension est incluse dans l’exécutable et préparée dans le dossier local de l’application sous `browser-extension`. Après une mise à jour, recharge-la depuis le navigateur. Ces étapes ne sont nécessaires que pour le secours.

## Dépannage

- Codex introuvable : vérifie le chemin et les permissions. Sous Windows, les scripts npm `.cmd` ne sont pas utilisés ; indique le binaire natif.
- Compte non connecté : utilise ChatGPT ; une clé API seule ne fournit pas les quotas de l’abonnement.
- Lecture impossible : vérifie Internet et la version de Codex, puis réessaie.
- Port 48721 occupé : le bridge de secours ne peut pas démarrer. La source directe continue de fonctionner.
- Linux sans zone de notification : utilise la fenêtre. Fermer quitte ; minimiser conserve le suivi.
- Windows : fermer masque la fenêtre si le tray est disponible ; **Quitter** termine l’application.

## Désinstallation

Quitte Quota Codex et désinstalle son paquet. Supprime aussi l’extension du navigateur si tu l’as installée. Codex CLI et sa connexion partagée ne sont pas supprimés.
