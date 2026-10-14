# Claude dans Quota Monitor

Le backend utilise le binaire officiel Claude Code, et non un client HTTP qui récupère ses credentials. Aucun prompt utilisateur ni requête de modèle n'est envoyé pour consulter les quotas.

## Connexion et réglages

Installer Claude Code et exécuter `claude auth login` dans son terminal. Dans Quota Monitor → Réglages → Fournisseurs, activer Claude. Aucun relais statusLine n'est nécessaire et les réglages Claude Code ne sont pas modifiés automatiquement. Le bouton de retrait d'un ancien relais reste disponible explicitement.

Réglages → Barre des menus : Codex, Claude ou les deux. Chaque fournisseur conserve son état et ses erreurs. Deux fournisseurs actifs sont affichés côte à côte dans le menu macOS ; un seul occupe sa largeur. La limite courte est visible ; les détails hebdomadaires et resets sont disponibles au survol de Détails… et au clic dans la fenêtre de détails.

## Transport

Codex : `codex app-server`, `account/rateLimits/read`.
Claude : `claude --safe-mode --strict-mcp-config --tools '' --input-format stream-json --output-format stream-json --verbose -p`, puis deux messages `control_request` : `initialize` et `get_usage`.

Claude Code 2.1.299 a été réellement vérifié. `get_usage` est expérimental : une version incompatible affiche une erreur explicite, jamais une donnée de démonstration. Chaque étape est bornée à 15 secondes, les lignes sont limitées à 1 Mio, stderr n'est pas publié ; le processus est fermé après la lecture. Seuls pourcentages et resets utiles sont conservés en mémoire, pas les identités du message d'initialisation. Les données restantes après une erreur conservent leur ancien horodatage ; les fenêtres expirées ne sont pas affichées comme valides.

Actualisation périodique : cadence des Réglages, minimum 15 s pour Claude. Actualiser Claude déclenche une nouvelle lecture ; Actualiser les quotas demande les deux fournisseurs. Désactiver Claude arrête les nouvelles lectures et invalide les réponses d'une génération antérieure. Il n'y a aucune dépendance Hermes ni SDK Python/Go à installer.

## Vérifications

- Tests Rust du protocole sur fixture : parsing, absence, zéro, date, échange structuré et fermeture du processus.
- Test réel opt-in : `cargo test --locked --manifest-path src-tauri/Cargo.toml claude_cli::tests::live_official_cli -- --ignored` (sans inférence).
- Exécutable packagé : `--claude-usage-smoke <dossier scratch>` appelle le transport de production et affiche uniquement les fenêtres de quotas.
- `--quota-ui-smoke <dossier PNG>` rend les vraies vues natives avec des données de démonstration, sans fenêtre visible (une et deux colonnes).

Les tests hors écran ne valident pas les gestes physiques de survol/clavier ni la réouverture/focus des fenêtres Réglages. Ceux-ci nécessitent une session graphique et l'accès d'accessibilité ; ne pas les déclarer réussis sur la base d'une compilation.
