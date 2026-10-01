# Installer Quota Codex

## Installer l’application

- Windows : lance `*-setup.exe`, puis Quota Codex depuis la page de fin ou le menu Démarrer.
- Linux Debian : `sudo apt install ./nom-du-paquet.deb`.
- Linux AppImage : rends le fichier exécutable puis lance-le ; FUSE peut être nécessaire.
- macOS : copie l’application du DMG dans Applications. L’application reste uniquement dans la barre des menus et n’ouvre pas de tableau de bord.

## Démarrage automatique sous Windows

L’installeur propose **Lancer Quota Codex à l’ouverture de session**, coché par défaut lors de la première installation. Décoche la case pour refuser. Les installations suivantes conservent le choix de l’installeur. L’enregistrement concerne uniquement l’utilisateur courant, sans droits administrateur.

À l’ouverture de session, l’application démarre dans la zone de notification, sans tableau de bord ni terminal. Clique sur son icône pour ouvrir le tableau de bord. Un lancement normal depuis le menu Démarrer ouvre toujours la fenêtre. Si la création de l’icône échoue, la fenêtre s’affiche pour garder l’application accessible.

Pour désactiver le lancement automatique ensuite : **Paramètres Windows → Applications → Démarrage → Quota Codex**. L’installeur ne réinitialise pas ce blocage Windows. La désinstallation supprime l’entrée de démarrage ; une mise à jour la conserve.

Installation silencieuse : `/S` utilise le choix enregistré, ou active le démarrage par défaut en l’absence de choix. `/S /AUTOSTART=0` désactive explicitement l’enregistrement ; `/S /AUTOSTART=1` l’active. Un blocage défini dans les paramètres Windows reste prioritaire.

Cette option d’installeur concerne Windows. Les paquets macOS et Linux ne modifient pas le démarrage automatique.

## Connexion directe à Codex

Codex CLI doit être installé séparément. Une installation compatible déjà connectée à ChatGPT suffit. Sur macOS, l’entrée **Connexion** apparaît dans le menu tant que le compte n’est pas connecté, puis disparaît automatiquement. Sur Windows et Linux, ouvre **Connexion** dans le tableau de bord. Un chemin complet peut être défini dans **Emplacement de Codex** ; sous Windows, choisis `codex.exe`.

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
