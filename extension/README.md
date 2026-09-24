# Quota Codex bridge

Extension Chromium Manifest V3 utilisée par l’application macOS Quota Codex.

Elle lit uniquement les limites visibles sur la page <https://chatgpt.com/codex/settings/usage> et les transmet à l’application locale via `http://127.0.0.1:48721/quota`. Elle ne demande aucun accès aux cookies, mots de passe ou stockage de ChatGPT.

La surveillance des demandes de rafraîchissement est assurée par le service worker de l’extension. Il peut ainsi recharger la page d’utilisation lorsqu’elle est suspendue en arrière-plan par Chromium ou Brave. Il signale aussi à l’application si la page est ouverte : quand la liaison disparaît, le widget repasse à `--` et un clic sur son icône rouvre la page. Seul le dernier identifiant numérique de rafraîchissement est conservé dans le stockage propre à l’extension.

Pour l’installation complète, consulte [le guide principal](../docs/INSTALLATION.md).
