# Installer et configurer Quota Codex

Ce guide installe deux éléments :

1. **Quota Codex**, le widget macOS qui vit dans la barre des menus ;
2. **Quota Codex bridge**, l’extension navigateur qui lit les quotas visibles dans ChatGPT.

L’un ne peut pas recevoir les données sans l’autre.

## 1. Télécharger les fichiers

Dans la section **Releases** du dépôt GitHub, ouvre la dernière version et télécharge :

- `Quota.Codex_1.0.0_universal.dmg` ;
- `quota-codex-extension-1.0.0.zip`.

Le fichier `universal` fonctionne sur les Mac Apple Silicon et Intel.

## 2. Installer l’application macOS

1. Double-clique sur le fichier `.dmg`.
2. Glisse **Quota Codex** dans le dossier **Applications**.
3. Éjecte l’image disque.
4. Ouvre le dossier Applications et lance **Quota Codex**.

L’application n’ouvre pas de fenêtre : une mascotte accompagnée d’un pourcentage apparaît directement dans la barre des menus.

### Si macOS refuse l’ouverture

Cette version communautaire n’est pas encore notariée par Apple :

1. ouvre **Applications** dans le Finder ;
2. fais un clic droit sur **Quota Codex** ;
3. choisis **Ouvrir** ;
4. confirme avec **Ouvrir** dans la boîte de dialogue.

Cette confirmation n’est demandée qu’au premier lancement.

## 3. Installer l’extension navigateur

Décompresse `quota-codex-extension-1.0.0.zip` dans un emplacement permanent, par exemple `Documents/Quota Codex`. Ne supprime pas ce dossier après l’installation : le navigateur en a besoin pour charger l’extension.

Ouvre ensuite la page correspondant à ton navigateur :

| Navigateur | Adresse des extensions |
| --- | --- |
| Google Chrome | `chrome://extensions` |
| Brave | `brave://extensions` |
| Microsoft Edge | `edge://extensions` |
| Arc | `arc://extensions` |

Puis :

1. active le **Mode développeur** ;
2. clique sur **Charger l’extension non empaquetée** ;
3. sélectionne le dossier `extension` qui contient `manifest.json` ;
4. vérifie que **Quota Codex bridge** est activée.

## 4. Récupérer les quotas

1. Vérifie que l’application Quota Codex est lancée dans la barre des menus.
2. Connecte-toi à ton compte sur <https://chatgpt.com>.
3. Ouvre directement <https://chatgpt.com/codex/settings/usage>.
4. Attends quelques secondes, puis clique sur le widget dans la barre des menus.

L’extension transmet immédiatement les limites détectées. Elle recommence lors d’une modification de la page et relit le DOM toutes les 5 secondes. Il n’est pas nécessaire de garder le menu du widget ouvert.

## 5. Comprendre les couleurs

- **Vert** : plus de 40 % restants ;
- **Orange** : de 21 à 40 % restants ;
- **Rouge** : 20 % restants ou moins.

Le pourcentage affiché à côté de la mascotte correspond à la limite de 5 heures.

## Mise à jour

### Application

Quitte Quota Codex, télécharge le nouveau `.dmg`, puis remplace l’application dans le dossier Applications.

### Extension

Remplace l’ancien dossier `extension` par le nouveau, puis ouvre la page des extensions du navigateur et clique sur l’icône **Recharger** de Quota Codex bridge.

## Dépannage

### Le widget affiche `--%` ou « En attente »

- lance d’abord l’application Quota Codex ;
- vérifie que l’extension est activée ;
- ouvre la page <https://chatgpt.com/codex/settings/usage> en étant connecté ;
- recharge cette page ;
- dans la page des extensions, clique sur **Recharger** pour Quota Codex bridge.

### Les données restent anciennes

Recharge la page d’utilisation ChatGPT. Une page entièrement fermée ne peut plus transmettre de nouvelles données ; les valeurs déjà reçues restent visibles jusqu’à la prochaine mise à jour.

### L’extension indique que l’application est indisponible

Le bridge local écoute uniquement sur `127.0.0.1:48721`. Vérifie que Quota Codex est lancé et qu’une autre copie de l’application n’est pas déjà ouverte.

### Le menu n’apparaît pas en plein écran

Le widget utilise un véritable menu natif macOS. S’il est masqué, vérifie dans **Réglages Système → Centre de contrôle** que la barre des menus n’est pas configurée pour rester cachée en permanence.

## Désinstallation

1. quitte Quota Codex depuis son menu ;
2. déplace l’application vers la Corbeille ;
3. supprime **Quota Codex bridge** depuis la page des extensions du navigateur ;
4. supprime le dossier décompressé de l’extension.
