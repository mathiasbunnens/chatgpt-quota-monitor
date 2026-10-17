# Claude dans Quota Monitor

Le backend utilise le binaire officiel Claude Code, et non un client HTTP qui récupère ses credentials. Aucun prompt utilisateur ni requête de modèle n'est envoyé pour consulter les quotas.

## Connexion et réglages

Installer Claude Code et exécuter `claude auth login` dans son terminal. Dans Quota Monitor → Réglages → Fournisseurs, activer Claude. Aucun relais statusLine n'est nécessaire et les réglages Claude Code ne sont pas modifiés automatiquement. Le bouton de retrait d'un ancien relais reste disponible explicitement.

Réglages → Barre des menus : Codex, Claude ou les deux. Chaque fournisseur conserve son état et ses erreurs. Sur macOS, l’application se limite au menu de la barre des menus et à la fenêtre Réglages : le menu affiche un en-tête d’état (À jour, Lecture…, Erreur) puis une carte par fournisseur actif avec toutes ses limites (5 heures, hebdomadaire, réserve), leur réinitialisation et les erreurs. Les deux fournisseurs utilisent les mêmes libellés et le même ordre : la limite courte puis la limite hebdomadaire, nommées par le temps restant avant réinitialisation (« Limite sur 1h38 », « Limite sur 7j2h ») ; l’abonnement (Plus, Pro…) s’affiche à côté du nom. Avec « Les deux », l’indicateur de la barre des menus empile « Codex » et « Claude » sur deux lignes, toujours sur la limite 5 heures. Il n’y a plus de fenêtre de détails : les liens d’usage détaillé (chatgpt.com, claude.ai), les réinitialisations Codex (icône et nombre restant, dans l’en-tête Codex) et l’usage supplémentaire Claude (indicateur « Extra » lorsqu’il est activé) se trouvent dans Réglages → Fournisseurs, fenêtre qui s’ajuste à son contenu.

## Dépannage Claude Code (macOS / Windows)

Quota Monitor doit retrouver le binaire Claude Code **et** lancer son protocole
`get_usage`. `claude auth login` seul ne suffit pas si le programme est introuvable.

- macOS : vérifiez `command -v claude`, `claude --version` et
  `claude auth status`. Les installations natives, Homebrew, nvm, fnm,
  Volta et asdf sont recherchées même lorsque Finder ne transmet pas le PATH
  du terminal. Le chemin de l'exécutable est ajouté en tête du PATH du
  sous-processus (utile pour un lanceur Node.js).
- Windows : la détection recherche désormais `claude.exe`, en particulier
  `%USERPROFILE%\.local\bin\claude.exe`. Les anciens lanceurs
  `claude.cmd` installés via npm ne sont pas pris en charge ici : préférez
  l'installation native.
- Si besoin, `QUOTA_CLAUDE_BINARY` accepte un chemin **absolu** vers
  l'exécutable ; il doit être présent dans l'environnement de Quota Monitor
  au lancement, et pas seulement dans celui du terminal.
- Claude reste affiché lorsqu'il est activé mais introuvable. Une erreur
  `get_usage` après détection peut signaler une incompatibilité de version
  ou une session API plutôt que l'absence de connexion à claude.ai.

## Transport

Codex : `codex app-server`, `account/rateLimits/read`.
Claude : `claude --safe-mode --strict-mcp-config --tools '' --input-format stream-json --output-format stream-json --verbose -p`, puis deux messages `control_request` : `initialize` et `get_usage`.

Claude Code 2.1.299 a été réellement vérifié. `get_usage` est expérimental : une version incompatible affiche une erreur explicite, jamais une donnée de démonstration. Chaque étape est bornée à 15 secondes, les lignes sont limitées à 1 Mio, stderr n'est pas publié ; le processus est fermé après la lecture. Seuls pourcentages et resets utiles sont conservés en mémoire, pas les identités du message d'initialisation. Les données restantes après une erreur conservent leur ancien horodatage ; les fenêtres expirées ne sont pas affichées comme valides.

Actualisation périodique : cadence des Réglages, minimum 15 s pour Claude. Actualiser Claude déclenche une nouvelle lecture ; Actualiser les quotas demande les deux fournisseurs. Désactiver Claude arrête les nouvelles lectures et invalide les réponses d'une génération antérieure. Il n'y a aucune dépendance Hermes ni SDK Python/Go à installer.

## Vérifications

- Tests Rust du protocole sur fixture : parsing, absence, zéro, date, échange structuré et fermeture du processus.
- Test réel opt-in : `cargo test --locked --manifest-path src-tauri/Cargo.toml claude_cli::tests::live_official_cli -- --ignored` (sans inférence).
- Exécutable packagé : `--claude-usage-smoke <dossier scratch>` appelle le transport de production et affiche uniquement les fenêtres de quotas.
- `--quota-ui-smoke <dossier PNG>` rend le vrai panneau du menu avec des données de démonstration, sans fenêtre visible (deux fournisseurs, erreur Codex, Claude en lecture, aucun fournisseur ; clair et sombre).

Les tests hors écran ne valident pas les gestes physiques de survol/clavier ni la réouverture/focus des fenêtres Réglages. Ceux-ci nécessitent une session graphique et l'accès d'accessibilité ; ne pas les déclarer réussis sur la base d'une compilation.
