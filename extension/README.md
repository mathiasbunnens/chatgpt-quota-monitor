# Quota Codex bridge

Extension Chromium Manifest V3 utilisée par l’application macOS Quota Codex.

Elle lit uniquement les limites visibles sur la page <https://chatgpt.com/codex/settings/usage> et les transmet à l’application locale via `http://127.0.0.1:48721/quota`. Elle ne demande aucun accès aux cookies, mots de passe ou stockage de ChatGPT.

La surveillance des demandes de rafraîchissement est assurée par le service worker de l’extension. Il conserve un onglet d’utilisation épinglé et inactif, puis le recharge périodiquement même lorsque Brave est en arrière-plan. Il signale aussi à l’application si la page est ouverte : quand la liaison disparaît, le widget repasse à `--` et un clic sur son icône relance Brave en arrière-plan avec cet onglet technique. Seuls l’identifiant de cet onglet et le dernier identifiant numérique de rafraîchissement sont conservés dans le stockage propre à l’extension.

Pour l’installation complète, consulte [le guide principal](../docs/INSTALLATION.md).
