# Audit strict — Sanaga face au niveau « 1 % mondial »

16 novembre 2026 · commit `34748ca` · version 0.4.0

**La question :** qu'est-ce qui empêche Sanaga d'être au niveau des meilleurs
navigateurs du monde (Chrome, Safari, Arc, Firefox) ?

**La réponse courte :** aujourd'hui Sanaga n'y est pas, et l'écart n'est pas
une question de finition. Il y a (1) des défauts qui cassent l'usage
quotidien ou la confiance — raccourcis clavier morts dans les pages, barre
d'adresse usurpable, fenêtre invité qui ouvre le profil du propriétaire,
favoris écrasés sur une erreur de lecture ; (2) des fonctions de base
absentes — progression des téléchargements, plein écran vidéo, restauration
de l'historique des onglets, liens `mailto:` ; (3) quatre plafonds
d'architecture qu'aucune correction ponctuelle ne lève — interface dessinée
par egui, raccourcis routés par du JavaScript injecté, fichiers TSV réécrits
en entier, moteur WebKitGTK ; (4) autour du code, un risque juridique écrit
noir sur blanc (`MECHANICS.md`) et aucun canal de mise à jour.

Les audits précédents (`docs/`) annoncent plusieurs points comme réglés qui
ne le sont pas ; ils sont listés en section 8.

## 1. Méthode et limites

- **Exécution réelle.** Sanaga compilé en release et en debug (Ubuntu 24.04,
  WebKitGTK 2.52.6, 4 cœurs, écran virtuel 1280×800, **rendu logiciel, pas de
  carte graphique**), puis mesuré et piloté avec de vraies frappes clavier.
- **Lecture du code.** Les 34 900 lignes de `src/` relues en six passes
  indépendantes (fenêtre et boucle d'affichage ; moteur et navigation ;
  données, sync, secrets ; barre d'outils et onglets ; réglages et fonctions
  secondaires ; tout ce qui entoure le code).
- **Niveau de preuve**, indiqué à chaque ligne :
  **[V]** vérifié en faisant tourner Sanaga ·
  **[C]** confirmé en lisant le chemin de code de bout en bout, non exécuté ·
  **[S]** suspecté : le code de Sanaga est lu, l'effet dépend de WebKit/GTK
  et n'a pas été reproduit.
- **Priorité :** P0 à corriger avant toute diffusion · P1 empêche l'usage
  quotidien sérieux · P2 écart net avec les meilleurs · P3 finition.
- **Non couvert :** vrai matériel avec carte graphique, Wayland/GNOME/KDE
  réels, sites réels (le bac à sable n'a pas d'accès web libre), lecteurs
  d'écran réels, appels vidéo. Les chiffres de dessin ci-dessous sont donc
  pessimistes ; les chiffres de réveils et de mémoire du moteur ne le sont pas.

## 2. Ce qui a été mesuré

| Mesure | Résultat | Ce que disent les docs |
|---|---|---|
| Ctrl+A puis Ctrl+B dans un `<textarea>` d'une page | **Rien ne se passe** : la page ne reçoit que « Control », la sélection reste vide (la lettre `x` tapée juste avant arrive bien) | — |
| Fenêtre de 600 px de large | Champ d'adresse écrasé : l'adresse affichée est « . », le compteur mémoire passe sous le bouton recharger (`docs/screenshots/audit-2026-11-16-600px.png`) | UX-AUDIT : « rien n'est coupé, à aucune largeur » |
| Mémoire, 1 onglet sur une page locale de 2 lignes | 426 Mo (interface 220, page 180, réseau 23) ; identique avec 100 onglets restaurés non montrés | AUDIT.md : interface 160 Mo |
| Repos, page immobile, 20 s | Fil principal : ~18 réveils/s ; 5,6 % d'un cœur (une image complète de la fenêtre redessinée chaque seconde, ici par le processeur) | GTK4.md : « 0 % », « ~1 image/s » |
| Écoute de l'instance unique | +20 réveils/s en permanence (boucle `accept` non bloquante + `sleep(50 ms)`, `src/shell.rs:1019-1027`) — lu, non mesuré ici (pas de `XDG_RUNTIME_DIR`) | — |
| Démarrage à froid | première image à ~1,3 s, moteur prêt à ~1,4 s, deux blocages de 170–420 ms dans les premières images (indicatif : rendu logiciel) | — |
| Binaire release | 29,8 Mo (strip), 481 paquets au `Cargo.lock`, deux piles graphiques (GTK 4 **et** eframe/winit) | — |
| Compilation | release 4 min 17, debug 7 min 34 (binaire debug : 593 Mo) | — |

## 3. Les bloquants (P0)

| # | Constat | Où | Preuve |
|---|---|---|---|
| B1 | **Les raccourcis dans une page passent par un script injecté, qui avale aussi ceux qu'il n'utilise pas.** Ctrl+A, B, I, Y, E, O, M sont annulés (`preventDefault` + `stopPropagation`) alors que `shortcut_name` n'en fait rien : tout sélectionner, gras, italique, rétablir sont morts dans Gmail, Docs, Notion, n'importe quel champ. Ctrl+S/K/P/F/D/H/J/U sont pris aux applications web sans condition. | `src/app/web.rs:86-99`, `138-189` | **[V]** |
| B2 | **Même mécanisme, autre conséquence :** le script n'est injecté que dans le cadre principal. Clavier dans une iframe d'une autre origine (lecteur YouTube, formulaire de paiement), page bloquée ou plantée : plus aucun raccourci, Ctrl+W ne ferme pas un onglet gelé. | `src/page.rs:1099`, `1171-1177` | [C] |
| B3 | **Raccourcis liés à la lettre produite, pas à la touche.** Clavier russe, grec, arabe, hébreu : Ctrl+C/V/T/W/L ne marchent ni dans l'interface ni dans les pages. En AZERTY, Ctrl+0 ne remet pas le zoom. | `src/shell.rs:1625-1640`, `756-795` | [C] |
| B4 | **Barre d'adresse usurpable.** Chaque `notify::uri` réécrit l'adresse affichée, y compris pour une navigation lancée par la page et pas encore aboutie : `location='https://banque.com:8443/attente'` affiche banque.com avec cadenas au-dessus du contenu du site malveillant. | `src/page.rs:1260` → `src/app/web.rs:451-466` | [C] côté Sanaga, [S] côté WebKit |
| B5 | **Le cadenas est `url.starts_with("https://")`.** Aucun état TLS n'est lu : après « Continuer (dangereux) » sur un certificat invalide, même cadenas qu'un site sain. Pas de « Non sécurisé » en http, pas de visionneuse de certificat, le cadenas n'est pas cliquable. | `src/app/toolbar.rs:513`, `523-525` | [C] |
| B6 | **Autorisations et boîtes de dialogue attribuées à `wv.uri()`** (même défaut que B4) : un site qui lance une navigation vers meet.google.com puis appelle `getUserMedia` peut hériter du « Toujours autoriser » caméra de Meet. | `src/app/plus.rs:70-81`, `296-298` | [C] code, [S] fenêtre de tir |
| B7 | **La fenêtre invité s'échappe.** `new_window()` ne transmet que `profil:`, jamais `invite:` : Ctrl+N depuis un invité ouvre une fenêtre sur le profil principal (cookies, favoris, historique, mots de passe), dans un second processus sans verrou. Pire : `follow_other_windows()` recharge `load_profile("")` — le profil principal — dans la fenêtre invité elle-même. Même trou pour `--app`. | `src/app/mod.rs:1539-1545`, `1692-1706` | [C] |
| B8 | **Une erreur de lecture efface les favoris.** `Err(_)` (pas seulement « fichier absent » : erreur disque, trop de fichiers ouverts, UTF-8 invalide) remplace les favoris par les cinq par défaut **et les écrit**. Aucune sauvegarde `.bak`. Avec la sync, la perte est propagée aux autres appareils comme une suppression. Même schéma pour réglages, historique, extensions. | `src/store.rs:702-713` (et 606, 617, 694) | [C] |
| B9 | **Plusieurs processus écrivent le même profil sans verrou** (invité, web-app, deux lancements simultanés, absence de `XDG_RUNTIME_DIR`), chacun réécrivant des fichiers entiers depuis sa mémoire ; nom temporaire fixe `<fichier>.tmp` avec suppression puis création → un fichier à moitié écrit peut être mis en place, ce qui déclenche B8. | `src/shell.rs:980`, `1011-1012` ; `src/store.rs:1097-1107` | [C] |
| B10 | **Navigation privée qui écrit sur disque et ne finit jamais.** Une seule session éphémère jusqu'à la fermeture de Sanaga (fermer tous les onglets privés ne déconnecte pas) ; autorisations, zoom, réglages de site et marqueurs de téléchargement d'un onglet privé sont écrits avec le nom du site. Résumé et traduction par IA envoient le texte d'un onglet privé au cloud sans garde. | `src/page.rs:965-977` ; `src/app/plus.rs:345-355` ; `src/app/extras.rs:102-108` ; `src/app/intelligence.rs:219-290` | [C] |
| B11 | **« Effacer les données » efface la mauvaise session** : celle du premier onglet qui a une page. Si c'est un onglet privé ou de conteneur, le profil principal garde cookies et cache, et le message dit « effacé ». Les conteneurs ne sont jamais effacés. Un clic, sans confirmation. | `src/app/plus.rs:441-456`, `479-481` | [C] |
| B12 | **Économiseur d'énergie déclenché par une souris sans fil.** `on_battery()` prend toute alimentation `type=Battery` en décharge sans vérifier `scope=Device` ; un PC fixe n'a pas d'entrée `Mains`. Réglage activé par défaut → au plus 3 pages vivantes, les autres tuées après 5 minutes, en permanence. | `src/app/tools.rs:523-544` ; `src/store.rs:291` | [C] |
| B13 | **Le moteur léger lit n'importe quel fichier local nommé par une page distante.** `<img src="file:///dev/zero">` en mode Lecteur : lecture sans limite dans le processus principal (celui qui a le trousseau), mémoire épuisée, Sanaga tombe. Ce processus n'est pas dans le bac à sable, contrairement à ce que dit le README. | `src/html.rs:613-617` → `src/net.rs:311-313` | [C] |
| B14 | **`MECHANICS.md` : rétro-ingénierie de Google Chrome documentée dans un dépôt public** (empreinte du `.deb`, adresses, désassemblage, chaînes de `fr.pak` reprises mot pour mot). Les conditions de Chrome l'interdisent ; tout ce qui a été pris est public dans Chromium (licence BSD). Détail en section 7. | `MECHANICS.md`, `mechanics.json`, commentaires de `src/app/plus.rs:244`, `tools.rs:518`, `settings.rs:1353`, `toolbar.rs:659` | [C] |
| B15 | **Aucun canal de mise à jour** pour un logiciel exposé à Internet : paquets à télécharger une fois, pas de dépôt, pas d'avis de nouvelle version, pas d'alerte quand WebKitGTK est ancien. | `docs/PRODUCTION.md:13,49` | [C] |

## 4. Fonctions de base absentes

Chacune est attendue par n'importe quel utilisateur dès le premier jour.

| Absent | Détail | Où |
|---|---|---|
| Progression des téléchargements | Un seul événement, « terminé ». Un ISO de 2 Go : rien à l'écran jusqu'à la fin. Ni pourcentage, ni annuler, pause, reprise, « demander où enregistrer », ni liste conservée après redémarrage. Dossier figé `~/Téléchargements` ou `~/Downloads`, sans lire les dossiers XDG. | `src/app/web.rs:615-622` ; `src/net.rs:125-136` |
| Plein écran vidéo | Aucun gestionnaire `enter-fullscreen` : la vidéo reste sous la barre d'outils et les onglets. F11 garde l'interface. [S] pour le rendu exact. | absent de `src/` |
| État des onglets | Un onglet = une adresse. Sommeil, restauration de session, « rouvrir l'onglet fermé », dupliquer, déplacer vers une fenêtre, reprise après plantage : historique précédent/suivant, défilement et formulaires perdus. `closed: Vec<String>`, perdu au redémarrage. | `src/app/nav.rs:95-135`, `642-646` |
| Liens externes | `mailto:`, `tel:`, `magnet:`, liens Zoom/Teams : la page est remplacée par une page d'erreur. | `src/page.rs:1286-1372` |
| Onglets en arrière-plan | Un lien ouvert au clic milieu ne charge rien tant qu'on ne l'affiche pas : ni titre, ni icône. | `src/app/web.rs:766-774` |
| Partage d'écran | `getDisplayMedia` refusé d'office (Meet, Teams, Jitsi). | `src/app/plus.rs:45-76` |
| Témoin caméra/micro | Un site autorisé peut enregistrer depuis un onglet caché sans aucun signe. | — |
| `beforeunload` | Fermer un onglet ou la fenêtre ne demande jamais « quitter sans enregistrer ? ». | `src/app/nav.rs:82-108` |
| Glisser-déposer | Aucun : ni lien vers la barre d'onglets, ni onglet hors de la fenêtre, ni fichier. | absent de `src/` |
| Protection anti-hameçonnage | Aucune liste (Safe Browsing ou équivalent). | — |
| HTTPS d'abord, DoH, proxy, certificats | Aucun réglage ; le client HTTP propre à Sanaga (moteur léger, source, listes, IA) **ignore le proxy système** → fuite d'adresse IP pour qui passe par un proxy ou Tor. | `src/net.rs:91-106` |
| Survol d'un lien | Pas d'adresse affichée au survol dans les pages WebKit. | — |
| Vraie barre de chargement | Barre qui boucle ; `estimated-load-progress` jamais lu. | `src/app/toolbar.rs:633-641` |
| Touches média / MPRIS, page qui ne répond plus, pop-up bloquée | Rien. | — |
| Ctrl+1…9, Ctrl+F5, Ctrl+G/F3, Alt+D, Ctrl+Maj+PgPréc/Suiv | Absents ; Ctrl+Maj+R ouvre le Lecteur au lieu de recharger sans cache. Non modifiables. | `src/app/nav.rs:572-619` |

## 5. Constats par domaine

### 5.1 Moteur et navigation

| P | Constat | Où | Preuve |
|---|---|---|---|
| P1 | Économiseur de mémoire : plafond dur et non annoncé de 12 / 6 / 3 pages vivantes (6 par défaut) ; la 7ᵉ page vue est tuée tout de suite. Non exemptés : envois de fichiers, discussions WebSocket, éditeurs sans formulaire (Docs, Figma), exports longs. Le réveil est un rechargement complet. | `src/app/plus.rs:246-281` | [C] |
| P1 | Fermer une pop-up ou endormir un onglet appelle `terminate_web_process` ; une pop-up liée partage le processus de la page qui l'a ouverte → après une connexion OAuth, la page d'origine peut rester morte, sans page d'erreur (`TerminatedByApi` ignoré). | `src/page.rs:1646-1655`, `1519-1525` | [S] à vérifier en premier |
| P1 | Détection « formulaire en cours » peu fiable (seul le dernier champ compte, cadre principal seulement) : un onglet avec du texte saisi peut être endormi. | `assets/passwords.js:85-93` | [C] |
| P2 | Titre périmé : une page sans `<title>` (image, PDF) garde le titre du site précédent, et entre dans l'historique sous ce titre. | `src/app/web.rs:482-494` | [C] |
| P2 | Identité (User-Agent) : chaînes figées Chrome 131 / Firefox 133 / Safari 18.2 (fin 2024), sans client hints ; le changement d'identité sur un lien d'iframe remplace tout l'onglet ; le moteur léger s'annonce « Sanaga/0.1 ». | `src/identity.rs:48-50` ; `src/page.rs:1350-1370` ; `src/net.rs:13` | [C] |
| P2 | Exceptions de certificat : proposées même pour les sites HSTS, valables par nom de site pour toute la session, sans révocation ni état visible. | `src/page.rs:340-346` | [C] / [S] |
| P2 | Bloqueur : tous les filtres cosmétiques abandonnés (emplacements de pub vides), `$document`, `$popup`, `$redirect`, `$removeparam` ignorés, deux listes seulement (pas de liste FR), pas de règles personnelles. La liste intégrée bloque `facebook.net` (connexion Facebook), `imasdk.googleapis.com`, `googletagmanager.com` sans exception possible. | `src/blocklist.rs:115`, `165-195` ; `src/site_prefs.rs:161-190` | [C] |
| P2 | Conteneurs : « A B » et « A-B » partagent le même dossier de cookies ; un clic sur × supprime sans confirmation ; recréer le nom dans la même session ressuscite les connexions ; dupliquer/rouvrir un onglet de conteneur l'ouvre dans le profil principal. | `src/app/containers.rs:48-64` ; `src/app/web.rs:251` | [C] |
| P2 | Historique écrit seulement en fin de chargement : une page qui ne finit jamais de charger n'y entre pas. | `src/app/web.rs:430-447` | [C] |
| P2 | « Arrêter » évalue `window.stop()` dans la page : sans effet quand la page est bloquée. | `src/app/nav.rs:365-376` | [C] |
| P2 | Lecteur, source et moteur léger refont la requête avec un autre client, sans cookies (page déconnectée), sans bloqueur. | `src/net.rs` ; `src/app/nav.rs:157-168` | [C] |
| P2 | Accès au stockage accordé d'office à Google, Apple, Microsoft, auth0, okta sur tous les sites. | `src/app/plus.rs:61-69` | [C] |
| P2 | Renvoi des sites protégés (DRM) : sans geste de l'utilisateur ni limite de débit ; `u` non validé passé à un autre navigateur sans `--`. | `src/page.rs:1294-1299` ; `src/identity.rs:292-293` | [C] |
| P2 | Réglages de site appliqués au début de la navigation, sur la page encore à l'écran (le mode sombre clignote). | `src/page.rs:1208` | [C] |
| P3 | Page hors ligne : boucle de rechargement toutes les 5 s si `navigator.onLine` diverge. Recherche dans la page : parcours complet du DOM à chaque résultat. Pas de préchargement DNS. Outils de développement toujours activés. | `src/app/errors.rs:142` ; `src/page.rs:58-109` | [C] |

### 5.2 Performance

| P | Constat | Où | Preuve |
|---|---|---|---|
| P1 | Suggestions de la barre d'adresse recalculées **à chaque image**, pas à chaque frappe : deux `to_lowercase()` par favori et par visite, plus `site_completion` qui parcourt 4 000 visites par frappe. C'est ce qui oblige à plafonner l'historique à 5 000 visites. Même schéma : palette, panneau latéral, page Historique. | `src/app/chrome.rs:107-179`, `252-267` | [C] |
| P1 | Chaque enregistrement d'une fenêtre fait recharger **tout le profil** à toutes les autres, dans le fil de la fenêtre (vidage des écritures, ~12 fichiers, 5 000 lignes). Avec deux fenêtres : à chaque page visitée. | `src/app/mod.rs:1692-1706` ; `src/store.rs:1205-1216` | [C] |
| P1 | Lectures disque à chaque image : `extensions::all()` relit tous les fichiers d'extension dans **toute** catégorie des Réglages ; `Store::profiles()`, `/etc/hostname`, `migrate::detect()` (page Import), `crash::list()` (20 fichiers), `stat` de l'avatar dans chaque fenêtre. | `src/app/settings.rs:911,922,1387` ; `importer.rs:127` ; `crashes.rs:102` ; `profiles.rs:75-81` | [C] |
| P1 | Chaque frappe dans un champ des Réglages (dossier de sync, page d'accueil, modèle) déclenche enregistrement + vérification des listes + tour de sync. Taper `/home/moi/Nextcloud` passe par `/home/moi`, où la sync crée `Sanaga/` et écrit favoris et onglets. | `src/app/settings.rs:1663-1672` | [C] |
| P2 | Une image de retard structurelle sur toute entrée continue : l'image egui tourne dans un `idle` (priorité 200), après le dessin de l'image en cours. | `src/shell.rs:66-72` | [C] |
| P2 | Un onglet qui charge, même caché, fait tourner toute l'interface au rythme de l'écran. | `src/app/mod.rs:2188-2191` | [C] |
| P2 | Réveils au repos : voir section 2. Images produites aussi quand la fenêtre est cachée ou réduite. | `src/shell.rs:837,1019` ; `src/app/web.rs:362-367` | [V] / [C] |
| P2 | Sync : un minuteur par fenêtre, toutes les 30 s, qui **écrit toujours** (nouveau chiffré à chaque fois → le cloud voit un fichier changé toutes les 30 s, pour toujours). | `src/app/accounts.rs:214-256` ; `src/sync.rs:312,396` | [C] |
| P2 | Chaque texture envoyée deux fois à la carte graphique (second peintre pour la couche des menus) ; ouvrir un menu sur une page réalloue un tampon de la taille de la fenêtre et s'affiche coupé pendant une image. | `src/shell.rs:1923-1971` ; `src/app/web.rs:641-653` | [C] |
| P2 | Icônes de sites : cache sans éviction, une analyse d'URL + un verrou par ligne et par image, un fil par icône. | `src/favicon.rs:67-141` | [C] |
| P2 | Source de la page quadratique (`to_ascii_lowercase` du reste du document à chaque `<script>`), dans le fil de la fenêtre. | `src/app/source.rs:64-67` | [C] |
| P2 | Listes non virtualisées : Historique (400 lignes puis « affinez la recherche »), favoris, mots de passe, 120 appareils. | `src/app/pages.rs:92-108` | [C] |
| P2 | Un fil par requête (≈ 29 points de création), sans réserve ni plafond ; `secret-tool` lancé une fois par opération, sans délai maximal. | `src/net.rs:175,306` ; `src/keyring.rs:11-62` | [C] |
| P3 | `CacheModel::DocumentBrowser` et limite mémoire RAM/8 : retour arrière plus lent, applications lourdes en pression mémoire permanente sur 8 Go. | `src/page.rs:852-869` ; `src/app/web.rs:845` | [C] / [S] |
| P3 | Enregistreur vidéo : cadre redessiné en cairo à chaque image dans le fil principal, tampons BGRA entiers clonés, `libx264` codé en dur (échoue sur Fedora). | `src/app/recorder.rs` | [C] |

### 5.3 Données, synchronisation, secrets

| P | Constat | Où | Preuve |
|---|---|---|---|
| P1 | Historique plafonné à 5 000 visites, tronqué en silence ; un import peut chasser l'historique existant. Pas de compteur de visites → pas de classement par fréquence possible. | `src/store.rs:423,696` | [C] |
| P1 | Sync sans versions ni pierres tombales : un fichier ancien remis par Nextcloud/Syncthing fait supprimer localement tout ce qu'il ne contient pas. Dossier non chiffré : qui peut y écrire peut effacer ou injecter des favoris `file://`. Ne couvre que favoris, liste de lecture, onglets. | `src/sync.rs:211-314` | [C] |
| P1 | Supprimer un profil laisse ses mots de passe, sa phrase de sync et sa clé Claude dans le trousseau, alors que le dialogue dit qu'ils sont effacés ; un nouveau profil du même nom en hérite. | `src/app/profiles.rs:373-383` | [C] |
| P1 | Mots de passe : « Afficher » et « Copier » sans ré-authentification ; le mot de passe affiché ne se masque jamais et peut s'afficher sur **la ligne d'un autre compte** après rafraîchissement (index conservé) ; presse-papiers jamais vidé. | `src/app/accounts.rs:258-339` | [C] |
| P1 | Remplissage des coordonnées dans des champs **invisibles** : une page qui montre un champ « nom » récupère e-mail, téléphone et adresse par des champs cachés. | `assets/passwords.js:158-172` | [C] |
| P2 | Motifs d'extension non ancrés : `*://*.banque.com/*` correspond à `https://pirate.example/?x=://a.banque.com/`. Les extensions tournent dans le monde de la page, cadre principal seulement, sans page d'arrière-plan : la plupart s'installent puis ne font rien. | `src/extensions.rs:200-227` ; `src/webext.rs:155-171` | [C] |
| P2 | Échecs d'écriture avalés (`let _ =`) : disque plein → plus rien n'est enregistré, sans avertissement. Pas de `fsync` du dossier après renommage. | `src/store.rs:1131-1141` | [C] |
| P2 | Aucune version de format dans les fichiers ; les clés inconnues sont perdues au prochain enregistrement (retour à une ancienne version = perte de réglages). | `src/store.rs:617-741` | [C] |
| P2 | Import : rien trouvé sur Ubuntu d'origine (Firefox et Chromium en snap/Flatpak non cherchés), profil `Default` seulement, dossiers de favoris aplatis, pas de mots de passe Firefox, bases copiées sans `-wal`. Import Chrome : les mots de passe de 32 caractères et plus sont probablement amputés. | `src/migrate.rs:46-115`, `146-215` | [C] / [S] |
| P2 | Chiffrement de la sync : sel régénéré en silence s'il manque (fichiers alors illisibles à jamais), pas de changement de phrase, Argon2 par défaut sans exigence sur la phrase, nom de machine et de profil en clair, deux machines de même nom s'écrasent. | `src/sync.rs:26-32`, `117-142` | [C] |
| P2 | IA : pas d'étape de consentement avant l'envoi du texte d'une page au cloud ; « Arrêter » n'interrompt pas la requête (facturée) ; texte de page injecté dans la consigne sans cadre ; écriture sur l'entrée de la commande avant de lancer la lecture → blocage définitif au-delà de 64 Ko ; `kill()` ne tue que l'enfant direct. | `src/ai.rs:140-150,262` ; `src/ai_cli.rs:140-214` | [C] |
| P3 | `--profile ../../..` non validé (chmod de dossiers hors de Sanaga) ; `crash.log` non nettoyé et sans limite ; `logins.tsv` en clair (sites + identifiants) ; note « en cours » trompée par un PID réutilisé. | `src/main.rs:206-210` ; `src/crash.rs:256-275` | [C] |

### 5.4 Barre d'adresse, onglets, menus

| P | Constat | Où | Preuve |
|---|---|---|---|
| P1 | La barre d'outils ne s'adapte pas à la largeur (constat [V] de la section 2) ; sélecteur d'onglets de 1 372 px fixe, dialogues de 440 px, palette de 560 px : inutilisable en fenêtre carrelée (Hyprland, le public visé). | `src/app/toolbar.rs:393-401` ; `switcher.rs:105-108` | **[V]** / [C] |
| P1 | Suggestions sans modèle de classement : ordre fixe, simple « contient », pas de fréquence, pas de suggestion de recherche, aucune ligne sélectionnée par défaut, pas de surlignage. | `src/app/chrome.rs:138-178` | [C] |
| P1 | Le champ garde le texte tapé puis abandonné ; Échap ne remet jamais l'adresse. Pas d'Alt+Entrée / Ctrl+Entrée, pas de menu contextuel, pas de « coller et aller ». | `src/app/toolbar.rs:567-631` | [C] |
| P1 | Adresse longue coupée **par la droite** : `login.banque.com.xxx.pirate.example` s'affiche `login.banque.com…`. Domaine réel non mis en valeur ; IDN toujours en punycode. | `src/app/toolbar.rs:517-520`, `663-693` | [C] |
| P1 | Adresse ou recherche ? `node.js`, `3.14`, `readme.md` → site https ; `routeur`, `nas/admin`, `[::1]:8080`, `exemple.com.` → recherche ; `192.168.1.1` → https ; `localhosting.com` → http. | `src/app/mod.rs:360-390` | [C] |
| P1 | Mots-clés codés en dur qui détournent des recherches : « m night shyamalan » → OpenStreetMap, « b vitamines » → Bing. Quatre moteurs fixes, aucun moteur personnalisé ; Wikipédia toujours en français. | `src/app/plus.rs:88-106` ; `src/store.rs:52` | [C] |
| P1 | Complétion vers la mauvaise origine : toujours `https://hôte` même si le site a été visité en `http://nas.lan:8080`. | `src/app/chrome.rs:252-267` | [C] |
| P1 | Bande d'onglets : activation au relâchement (Chrome : à l'appui) ; largeurs ré-animées à chaque fermeture (la croix suivante fuit sous le curseur) ; glisser = échange d'index sans que l'onglet suive ; pas de sélection multiple ; fermeture d'un groupe en un clic sans annulation. | `src/app/tabbar.rs:73,268-322` | [C] |
| P1 | Confirmation « appuyer deux fois en 4 s » : un double-clic sur « Effacer l'historique » efface tout, sans annulation. | `src/app/chrome.rs:47-67` | [C] |
| P2 | Favoris : liste plate (pas de dossiers ; les imports sont aplatis) ; la barre abandonne en silence ce qui ne tient pas ; double-clic pour renommer inatteignable (le premier clic navigue) ; l'infobulle de l'étoile dit « Retirer » mais ouvre la bulle. | `src/store.rs:337-340` ; `src/app/menus.rs:42-44` ; `bookmarks.rs:254-260` | [C] |
| P2 | Menu principal : « Plus d'outils » = 25 lignes à plat, dépasse une fenêtre de 800 px, dernière ligne coupée ; mêmes icônes pour des choses sans rapport ; pas de navigation aux flèches, aucune touche pour l'ouvrir. | `src/app/mainmenu.rs:99-158` ; `docs/screenshots/menu.png` | [C] |
| P2 | Dialogues JavaScript : Entrée/Échap lus globalement — Entrée tapée dans la barre d'adresse répond au dialogue ; pour `beforeunload`, Entrée = Quitter. | `src/app/dialogs.rs:62,146-152` | [C] |
| P2 | Titres d'onglets : ~48 px de texte à la largeur minimale, coupés net sans « … » pour les onglets inactifs (défaut P1 de l'audit du 11/11, annoncé réglé). | `src/app/tabbar.rs:22,197-239` | [C] |
| P2 | Onglets en arbre : le glisser ne change jamais le parent ; `move_tab` appelé à chaque image ; pas d'indicateur de dépôt. | `src/app/layout.rs:290-298` | [C] |
| P2 | Nouvel onglet : un tiers de l'écran pour un « rapport de sobriété » qui montre des compteurs de débogage ; tuiles sans menu contextuel ni clic milieu ; « Fréquents » compte des adresses exactes. | `src/app/start.rs` | [C] |
| P2 | Accueil en sept étapes avant la première page ; « Importer maintenant » quitte la visite ; Firefox non détecté à cette étape. | `src/app/onboarding.rs` | [C] |
| P2 | Favoris par défaut = cinq sites du propriétaire (visible sur la capture), alors que `docs/AUDIT.md:42` écrit « aucune tuile sponsorisée ». | `src/store.rs:1254-1266` | **[V]** |
| P3 | Deux systèmes de menus (30 px / 26 px), trois mécanismes d'annulation/notification, tailles d'icônes 17/12/11 px, trois formes de survol, pas d'état « appuyé ». Hauteur d'interface ~118 px pour un seul onglet. Jauge mémoire, simulateur mobile et ✦ dans la barre par défaut. | divers | [C] |

### 5.5 Réglages et fonctions secondaires

| P | Constat | Où | Preuve |
|---|---|---|---|
| P1 | Traduction par IA bloquée pour toujours sur des pages ordinaires : le script remplit des lots jusqu'à 5 000 caractères + un paragraphe, le lecteur refuse au-delà de 6 000 et sort sans rien dire. | `src/app/translate.rs:32,112,423` | [C] |
| P1 | Traduction par IA : le filtre n'admet que latin, cyrillique, arabe et idéogrammes. Page coréenne, hindi, grecque, hébraïque : zéro texte traité, et la barre annonce « Page traduite ». Japonais : kana ignorés. | `src/app/translate.rs:23` | [C] |
| P1 | « Charger les images » ne fait rien avec le moteur par défaut (lu seulement par le moteur léger). | `src/app/settings.rs:1365` | [C] |
| P1 | La note « les sites sont demandés dans la langue choisie » est fausse pour WebKit (aucune langue préférée n'est réglée). | `src/app/settings.rs:963` | [C] |
| P2 | Simulateur mobile : n'émule que la taille et le User-Agent. Pas de densité de pixels, pas d'événements tactiles, `(pointer:coarse)` reste souris, pas de zones sûres ni de limitation réseau ; recharge la page à l'entrée **et** à chaque changement d'onglet. | `src/app/devices.rs:27-36`, `169-175`, `454-491` | [C] |
| P2 | Traduction sur place fragile : nœud par nœud (ordre des mots perdu), pas de suivi des contenus chargés ensuite, pas de `dir=rtl`, une réponse de mauvaise longueur décale tous les textes suivants ; état global (lancer sur B abandonne A à moitié traduit). | `src/app/translate.rs:14-63`, `440-455` | [C] |
| P2 | Offre de traduction : se fie à `<html lang>`, revient à chaque page, pas de « jamais pour cette langue ». | `src/app/translate.rs:222-235` | [C] |
| P2 | Bandeau d'autorisation : phrase fautive dans toutes les langues pour les notifications (« x souhaite accéder à vous envoyer des notifications ») ; « Bloquer » toujours mémorisé ; une demande à la fois, affichée sur l'onglet actif quel qu'il soit. | `src/app/plus.rs:286-357` | [C] |
| P2 | « Toujours au premier plan » ne marche que sur Hyprland mais est proposé à tous. Vidéo flottante : YouTube rouvert en `youtube-nocookie` (connexion, qualité, sous-titres perdus), échec sur Twitch/Vimeo. | `src/desktop.rs:185-207` ; `src/app/tools.rs:345-383` | [C] |
| P2 | Supprimer un élément : cliquer le fond masque `<body>` (page blanche) ; règle par site bâtie sur `nth-of-type`, qui masque autre chose ailleurs sur le site ; pas d'annulation. | `src/app/tools.rs:29-42`, `399-415` | [C] |
| P2 | Réglages : remplissage des coordonnées rangé sous « Mots de passe » ; trois surfaces « par site » avec deux mots différents ; « Sécurité » = deux coches non interactives ; un changement s'applique tantôt tout de suite, tantôt aux prochains onglets, sans le dire. Panneau du site : chaque interrupteur recharge la page. | `src/app/settings.rs` ; `sitepanel.rs:189` | [C] |
| P2 | Réglages IA : « Ollama ne répond pas » affiché même quand les modèles sont listés ; modèle Claude en texte libre ; clé jamais testée ; aucun coût affiché. | `src/app/settings.rs:1139,1157` | [C] |
| P2 | Gestionnaire de mots de passe squelettique : ni modifier, ni ajouter, ni exporter, ni générateur, ni « jamais pour ce site » ; origine exacte seulement ; au plus 3 comptes proposés ; sur un formulaire de changement de mot de passe, c'est l'ancien qui est enregistré. | `src/app/accounts.rs` ; `assets/passwords.js:51-70` | [C] |
| P3 | Bordure animée de fenêtre : toute l'interface repeinte à 15 images/s pour toujours. Renvoi DRM déclenché à chaque image tant que la condition tient. | `src/app/tools.rs:142` ; `media.rs:144-167` | [C] |

Réglages attendus et absents : moteurs de recherche personnalisés, pages de
démarrage, emplacement des téléchargements, langues des sites et
dictionnaires, taille minimale de police, proxy, DNS chiffré, HTTPS seul,
GPC, visionneuse de cookies par site, effacement à la fermeture, valeurs par
défaut des autorisations, gestion des certificats, cartes et adresses
multiples, personnalisation des raccourcis, « toujours garder ce site
actif ».

### 5.6 Accessibilité, langues, intégration au bureau

| P | Constat | Où | Preuve |
|---|---|---|---|
| P1 | Pas de pré-édition des méthodes de saisie : chinois, japonais, coréen et touches mortes tapés à l'aveugle dans tous les champs de Sanaga. | `src/shell.rs:1413-1423` | [C] |
| P1 | Pas de mise en forme du texte ni de bidirectionnel : titres arabes et hébreux en lettres détachées à l'envers, conjonctions indiennes et thaï cassées. Polices cherchées par chemins codés en dur, pas par fontconfig (khmer, télougou, tibétain… : carrés vides). | egui 0.37 ; `src/main.rs:63-117` | [C] |
| P1 | Le clavier ne revient pas à la page après un clic sur l'interface : cliquer Recharger puis Espace ou PgSuiv ne fait rien. | `src/shell.rs:1579-1581`, `1844-1852` | [C] |
| P1 | Anneau de focus absent hors Réglages (onglets, étoile, favoris, panneau latéral, tuiles, menus) : Tab parcourt l'interface sans qu'on voie où l'on est. L'audit du 11/11 dit le contraire. | usages de `focus_ring` | [C] |
| P1 | Lecteurs d'écran : lignes de menu, suggestions, palette sans nom ; onglets en « bouton » sans état sélectionné ; deux arbres d'accessibilité sans lien (interface / pages) ; notifications non annoncées. `PRODUCTION.md` le marque ✅. | `src/shell.rs:669-714`, `1789-1791` | [C] / [S] |
| P1 | Échelle fractionnaire et « Grand texte » ignorés (`scale_factor()` entier) : à 125–150 % l'interface est rendue en 2× puis ré-échantillonnée ; pas de réaction au changement d'écran. Police Inter imposée, pas celle du système. | `src/shell.rs:1725,1977` | [C] / [S] |
| P1 | Fenêtre sans décoration : les bords situés sous la page ne permettent pas de redimensionner sous GNOME/KDE ; pas d'ombre ; feux tricolores macOS toujours à gauche, sans lire `gtk-decoration-layout` ; avec les onglets verticaux ils se retrouvent au milieu du bord supérieur. | `src/shell.rs:1314` ; `src/desktop.rs:332-362` ; `toolbar.rs:80-114` | [C] / [S] |
| P2 | Redimensionnement : l'interface suit avec une ou deux images de retard et une bande noire sur le bord qui grandit. | `src/shell.rs:1478-1487`, `1987-1999` | [C] |
| P2 | Pas de sélection primaire (clic milieu pour coller), pas de défilement inertiel dans l'interface, pas de tactile ni de pincement. | `src/shell.rs:1553-1650` | [C] |
| P2 | Plusieurs fenêtres : icônes de sites fausses ou absentes dans toute fenêtre après la première (identifiants de texture propres à chaque contexte) ; état « maximisé » global au processus ; un lien venu d'une autre application peut ne pas s'ouvrir, ou s'ouvrir derrière (pas de jeton d'activation). | `src/favicon.rs:31,92` ; `src/desktop.rs:216` ; `src/shell.rs:111-116` | [C] |
| P2 | Échec de l'accélération graphique : fenêtre vide pour toujours, sans message. Reprise après panique : les textures de l'image perdue ne sont jamais envoyées (texte abîmé jusqu'à la fermeture) ; une panique dans un signal GTK/WebKit termine quand même Sanaga. | `src/shell.rs:1435-1461`, `1802-1816` | [C] |
| P2 | `std::env::set_var("LANGUAGE")` à chaque nouvelle fenêtre, alors que des fils tournent : comportement indéfini, plantage rare sur Ctrl+N. | `src/app/mod.rs:816-818` | [C] |
| P2 | Dialogues de fichiers : boucle principale imbriquée dans l'image ; une fermeture pendant le dialogue saute l'enregistrement de la session. | `src/shell.rs:621-651` | [C] |
| P2 | Contrastes sous AA (onglets inactifs ≈ 3,9:1, nom de groupe jaune ≈ 2,9:1) ; cibles de 12 à 22 px (croix d'onglet 18 px). | `src/app/tabbar.rs:246,259` ; `layout.rs:200,240` | [C] calculé |
| P2 | Traductions : le texte français sert de clé (corriger une faute en français casse quatre langues ; « OK » devient « Done ») ; pas de pluriels (« 1 months ago », « {} onglet(s) ») ; pas de paramètres réordonnables ; nombres toujours à virgule ; chaînes non traduites (« Google Traduction », « (sans identifiant) », `--profile Travail` dans le lanceur) ; messages de la ligne de commande en français. | `src/i18n.rs` ; `src/store.rs:1279` ; `src/main.rs` | [C] |
| P3 | Taille et position de fenêtre non mémorisées (toujours 1280×800) ; titre de fenêtre mis à jour une image sur dix ; pas de `--version` ni de fenêtre privée en ligne de commande ; réduction des animations respectée seulement dans les Réglages. | `src/shell.rs:1312` ; `src/app/mod.rs:2022` | [C] |

## 6. Les plafonds d'architecture

Ces points ne se corrigent pas ligne par ligne ; ils demandent une décision.

1. **L'interface dessinée par egui.** Pas de mise en forme du texte, pas de
   fontconfig, pas de surface de méthode de saisie, un second arbre
   d'accessibilité, pas de glisser-déposer ni de sélection primaire natifs,
   mode immédiat (toute l'application rejouée à chaque mouvement de souris).
   Les navigateurs du premier rang ont une pile de texte complète. Deux
   voies : champs de texte et menus en widgets GTK natifs au-dessus du
   dessin, ou intégrer HarfBuzz + fontconfig + bidi dans le rendu.
2. **Les raccourcis par JavaScript injecté** (B1–B3). À déplacer dans un
   contrôleur de touches GTK en phase de capture, par code de touche.
3. **L'interface en une texture sous les pages.** Tout ce qui passe
   au-dessus d'une page demande une seconde couche ou une image figée ;
   bords de fenêtre, curseurs et infobulles au-dessus de la page en
   souffrent.
4. **Le stockage en fichiers TSV.** Réécriture complète, copie en mémoire
   par fenêtre, rechargement total comme seule invalidation, pas de
   transactions, de versions ni de sauvegardes. SQLite est déjà une
   dépendance : un magasin en mode WAL, détenu par un seul processus par
   profil, lève d'un coup le plafond de 5 000 visites, permet le classement
   par fréquence et supprime B8, B9 et le rechargement entre fenêtres.
5. **Le modèle « un état par fenêtre + canaux globaux »** (`HOST`,
   `favicon::STATE`, `MAXIMIZED`, `ACTIVE_WINDOW`) : la justesse à plusieurs
   fenêtres demande un modèle partagé. `struct Sanaga` a ~206 champs répartis
   sur 41 fichiers, ce qui rend la logique des onglets intestable.
6. **WebKitGTK.** Pas de Widevine (Netflix, Spotify web), pas d'API
   WebExtensions (ni uBlock Origin ni gestionnaire de mots de passe), pas
   d'isolation de sites, pas de gel d'onglet (vivant ou tué), correctifs de
   sécurité au rythme de la distribution, compatibilité en retrait sur les
   sites testés seulement sous Chrome. Aucun navigateur WebKitGTK n'est
   aujourd'hui dans le premier rang ; ce plafond-là reste, quelle que soit
   la finition.

## 7. Hors du périmètre du code — approfondi

### 7.1 Juridique

- **`MECHANICS.md` (B14).** Ce qui a été pris dans le binaire de Chrome : la
  formule de l'économiseur de mémoire (6 h − 2 h × niveau), la table de
  revisites 5/15/15, le seuil de batterie de 20 %, les libellés français
  « Modéré / Équilibré / Maximal » et « Toujours afficher les URL en
  entier », mot pour mot. Les nombres sont des faits, non protégeables ; les
  libellés viennent d'un fichier de langue propriétaire ; et le document
  lui-même est l'aveu, daté et précis, d'un désassemblage que les conditions
  de Chrome interdisent. Tout est disponible dans Chromium sous licence BSD
  (`memory_saver_mode_policy.cc`, `battery_saver_mode_manager.cc`, fichiers
  `.xtb`). À faire : retirer le fichier, `mechanics.json` et les adresses
  citées dans les commentaires, citer Chromium à la place, et demander un
  avis juridique sur l'historique git (le fichier y reste).
  *Je ne suis pas juriste ; c'est un risque à faire évaluer, pas un verdict.*
- **Licences tierces.** Aucun paquet n'embarque les notices des ~370 crates
  ni la licence OFL des polices Inter et Noto (obligatoire avec chaque
  copie). `eframe/default_fonts` embarque en plus Ubuntu-Light et Hack, non
  mentionnés. Pas de page `sanaga://licenses`, pas de `cargo-deny`.
- **Marques et apparence.** « Safari-inspired », « Chrome-style »,
  « macOS-27-style » dans le README ; silhouettes d'iPhone dans les captures.
  Pas de logo copié (cadres dessinés en code) : risque faible à modéré,
  à neutraliser dans le texte.
- **Identité Chrome usurpée par défaut** pour WhatsApp, Teams, Zoom,
  Messenger : contraire aux conditions de ces services, et détectable.

### 7.2 Diffusion et sécurité dans la durée

- Pas de mise à jour (B15) ; sur aucun canal (ni Flathub, ni AUR, ni dépôt
  apt/dnf) ; x86_64 seulement.
- Les binaires release ne sont jamais ouverts avant publication : tous les
  tests de fenêtre tournent sur le binaire debug, les crochets de test
  n'existant qu'en debug.
- `strip = true` sans symboles conservés : un plantage dans GTK, WebKit ou
  un bloc `unsafe` (28 dans `src/`, 21 dans `shell.rs`) ne laisse que
  « arrêt brutal ».
- Paquets : `.deb` sans dépendances calculées ni version minimale de
  WebKitGTK (le code exige 2.48) ; `launchable` AppStream qui ne correspond
  pas au `.desktop` installé ; action « Travail » en français pour tous ;
  PKGBUILD avec `sha256sums=('SKIP')` ; manifeste Flatpak non recevable sur
  Flathub ; `install.sh` qui écrit `Exec=` sans guillemets et ne vérifie pas
  `bwrap`.
- Intégrité : pas de SBOM, pas de signature des paquets ni des étiquettes,
  chaîne d'outils flottante, `pip install` non épinglé dans la tâche Flatpak.
- `SECURITY.md` : pas d'adresse de contact, pas de délai de divulgation.
- `docs/SECURITY-AUDIT-2026-11-08.md` dit lui-même que le navigateur n'a pas
  été lancé pour vérifier ses correctifs, et sa liste de 20 contrôles n'a
  pas de résultats consignés.

### 7.3 Tests

- 110 tests pour 34 900 lignes. **Zéro** dans `shell.rs`, `page.rs`,
  `web.rs`, `nav.rs`, `tabs.rs`, `groups.rs`, `tabbar.rs`, `sidebar.rs`,
  `containers.rs`, `accounts.rs`. Restauration de session : un test.
  Classement adresse/recherche : un test. Sync : trois.
- Aucune comparaison d'images, aucune mesure de performance suivie : le seul
  garde-fou est « pas d'image de plus de 500 ms » sur un binaire debug en
  640×480. Les chiffres publiés (« 0 % », « 510 Mo pour 10 onglets ») n'ont
  ni script ni matériel consigné — et les mesures de la section 2 ne les
  retrouvent pas.
- WebDriver : un seul scénario heureux ; il ne voit ni l'interface egui ni
  les pages `sanaga://`. L'interface propre de Sanaga n'a donc aucun test
  fonctionnel automatique.
- Fuzzing : corpus non conservé (chaque passe repart de zéro) ; non
  couverts : lecteurs de `store.rs`, `sync.rs`, `migrate.rs`, décodage
  d'images. `rustfmt` et la version minimale de Rust (1.88) ne sont pas
  vérifiés en CI ; `cargo audit` ne tourne pas sur horaire.

### 7.4 Projet

- **Un seul auteur, 101 commits en cinq jours** (8–12 novembre), versions
  0.1.0 à 0.4.0 en deux jours, commits jusqu'à ~23 000 lignes, trois PR
  fusionnées par leur auteur, commits non signés. Aucune distribution ni
  entreprise n'adoptera un navigateur avec cette provenance sans relecture
  indépendante.
- **Étalement.** Un moteur HTML maison, cinq fournisseurs d'IA, un écrivain
  d'extensions, un simulateur mobile avec enregistrement vidéo, un
  déchiffreur de mots de passe Chrome, une sync par dossier, des onglets en
  arbre, la vue partagée : chacun existe, aucun n'est profond, et les bases
  de la section 4 manquent. Le premier rang se gagne sur les bases.
- **Réglé sur un seul bureau.** Premier plan, vidéo flottante, déplacement
  de fenêtre : comportement complet sur Hyprland seulement ; GNOME et KDE,
  la majorité, reçoivent le chemin dégradé.
- **Deux piles graphiques** liées dans le binaire : `eframe`/winit n'est là
  que pour un repli sans moteur web.

## 8. Ce que les documents affirment et que le code contredit

| Document | Affirmation | Réalité |
|---|---|---|
| `UX-AUDIT` S1/P1 | « Rien n'est coupé, à aucune largeur » | Barre d'adresse illisible à 600 px **[V]** ; titres d'onglets inactifs coupés net |
| `UX-AUDIT` | « Tab atteint chaque contrôle, chacun avec un anneau de focus » | Anneau seulement sur quatre types de contrôles et les Réglages |
| `GTK4.md`, `AUDIT.md` | « 0 % au repos », « ~1 image/s » | ~18 réveils/s du fil principal, +20/s pour l'instance unique **[V]/[C]** |
| `AUDIT.md:42` | « Aucune tuile sponsorisée » | Cinq favoris par défaut = sites du propriétaire **[V]** |
| `AUDIT.md:14` | Liste des « seules requêtes » émises | Omet l'API Anthropic, youtube-nocookie, translate.google |
| `README.md:43` | « Les pages tournent dans un bac à sable » | Faux pour le moteur léger et le Lecteur (processus principal) |
| `PRODUCTION.md` | Accessibilité ✅ | Champs sans nom, ordre de focus non défini — listés « à suivre » dans la même ligne |
| `PRODUCTION.md` / `GTK4.md` / `DISTRIBUTIONS.md` | Trois versions de ce qui se passe sans `bwrap` | Non testé ; `SANAGA_NO_SANDBOX` documenté mais absent du code |
| `README.md:188` | Paquets « construits à chaque push » | Sur étiquette seulement (`release.yml`) |
| `STABILITY-AUDIT` | « Plus rien n'est attendu dans le fil de la fenêtre » | Rechargement du profil, lectures disque par image, sockets Hyprland, dialogues de fichiers (5.2, 5.6) |
| Réglages | « Les sites sont demandés dans la langue choisie » ; « Charger les images » | Sans effet avec WebKit |
| `src/store.rs:85` | Commentaire « 60 / 20 / 5 min » | Code et interface : 6 / 4 / 2 h |

## 9. Ordre recommandé

1. **Arrêter l'hémorragie (jours).** B1–B3 (raccourcis dans un contrôleur
   GTK), B4–B6 (adresse validée, cadenas d'après l'état TLS), B7 (invité),
   B8–B9 (ne jamais écraser un fichier illisible, verrou de profil, nom
   temporaire unique), B10–B13. Retirer `MECHANICS.md`.
2. **Les bases d'un navigateur (semaines).** Téléchargements, plein écran,
   état de session par onglet (`session_state()` de WebKit), liens
   externes, chargement en arrière-plan, `beforeunload`, Ctrl+1…9, barre
   d'outils qui s'adapte, retour du clavier à la page.
3. **Le socle de données.** Passage à SQLite, un propriétaire par profil,
   historique sans plafond, classement par fréquence, suggestions calculées
   par frappe hors du fil de la fenêtre.
4. **Décision sur le texte.** Widgets natifs ou pile de texte complète :
   sans elle, pas de saisie asiatique, pas d'arabe ni d'hébreu, pas
   d'accessibilité réelle.
5. **La confiance.** Canal de mise à jour, publication sur Flathub/AUR,
   tests sur le binaire release, mesures de performance suivies en CI sur du
   vrai matériel, relecture par une seconde personne, notices de licences.
6. **Élaguer.** Marquer comme expérimental ou retirer ce qui n'est pas
   profond (moteur léger, simulateur, écrivain d'extensions) tant que 1–5 ne
   sont pas faits.

Le plafond WebKitGTK (section 6, point 6) reste après tout cela : le
positionnement honnête aujourd'hui est « navigateur Linux léger et soigné »,
pas « premier rang mondial », tant que DRM, extensions et compatibilité ne
sont pas réglés par un autre moteur ou par WebKitGTK lui-même.
