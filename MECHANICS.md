# Mechanics taken from the original

Values and rules read in the original (Google Chrome for Linux, the program
`opt/google/chrome/chrome` of `google-chrome-stable_current_amd64.deb`,
SHA-256 `fa67e06a5bcd0525739996fb8e3ecff449039e6165c15ea7968d3935e59e829e`),
with where they were found, and how Sanaga uses them. Addresses are the
program's virtual addresses.

How they were read: the program is stripped and 307 MB, so the built-in
engine didn't discover these functions (`native_function_find` gave "not
inside any discovered function") and `native_function_pseudocode` couldn't
be used on them. They were found from their texts (`native_strings_find`),
then from the instructions that reach those texts (`lea [rip+…]` and `call`
scans of the code), and read as machine code with objdump.

## Memory saver (sleeping tabs)

### Three levels — observed
Chrome's Settings page (resource 22297 of `resources.pak`) offers three
levels, stored in the preference
`performance_tuning.high_efficiency_mode.aggressiveness`:
`CONSERVATIVE = 0`, `MEDIUM = 1`, `AGGRESSIVE = 2`. Their French names
(`locales/fr.pak`, resources 6542–6547): **Modéré**, **Équilibré
(recommandé)**, **Maximal** — "tabs become inactive after a long / an
optimal / a short period".

Sanaga: the same three levels and names (Settings › Performance ›
Économiseur de mémoire), `memory_saver` 1, 2, 3.

### Delay before an inactive tab sleeps — observed
Two functions compute it and pass it to `StartDiscardTimerIfEnabled`
(`0x77ec8b0`, named by the text `0x1e4f402` and the source path
`0x1e86263`, `chrome/browser/performance_manager/policies/memory_saver_mode_policy.cc`, line 174):

- `0xb734420` (a tab becomes eligible), at `0xb73445a`
- `0xb7344d0` (every tab, when the level changes), at `0xb734527`

Both do:

```
level = this->mode            // [rdi+0x60], 0..2 (checked < 3)
delay = 0x50775d800 + level × 0xfffffffe52d8b800   // microseconds
      = 21 600 000 000 − level × 7 200 000 000 µs
      = 6 h − 2 h × level
```

| Level | Delay |
|---|---|
| Conservative / Modéré (0) | **6 hours** |
| Medium / Équilibré (1) | **4 hours** |
| Aggressive / Maximal (2) | **2 hours** |

Sanaga: `src/app/plus.rs` `memory_saver_step` — 360, 240 and 120 minutes.

### Often revisited tabs stay awake — values observed, meaning inferred
In `StartDiscardTimerIfEnabled` (`0x77ec8b0`), at `0x77ec92a`–`0x77ec947`:

```
record = per-tab record copied by 0xe7f5870 (48 bytes)
limit  = table_0x2b42138[level]          // bytes 5, 15, 15
if record.field_0x28 > limit: return     // no timer for this tab
```

The table at `0x2b42138` holds **5, 15, 15** (Conservative, Medium,
Aggressive). The program also holds the text `num_revisits` (`0x1b7876b`):
the compared field is most likely the number of times the tab was revisited
(inferred, not proven from the code alone).

Sanaga: a tab shown again counts one revisit (`Tab::revisits`); the timer
doesn't put to sleep a tab revisited more than 5 / 15 / 15 times.

### Sanaga's own rules (not Chrome's)
- At most 12 / 6 / 3 pages running at once (the least recently seen sleep
  first). Chrome has no such count; it relies on its timer and on memory
  pressure.
- Energy saver mode (on battery or always): inactive tabs sleep after
  **5 minutes**, at most 3 pages running. Chrome's energy saver doesn't put
  tabs to sleep, so there is no Chrome value for it.
- Never asleep: the tab shown, a page playing sound, a call, a pinned tab, a
  form being filled in.

## Energy saver — observed

Chrome's energy saver (`battery_saver_mode`, managed in
`chrome/browser/performance_manager/user_tuning/battery_saver_mode_manager.cc`)
has four states in its Settings page (resource 22297 of `resources.pak`):
`DISABLED`, `ENABLED_BELOW_THRESHOLD`, `ENABLED_ON_BATTERY = 2`,
`ENABLED = 3`.

The threshold is written into the label "Activer uniquement lorsque le
niveau de la batterie est inférieur ou égal à $1 %" (`fr.pak` 6561, message
`0x19a1`, name `batterySaverModeEnabledBelowThresholdLabel` at `0x1ce5170`).
The code that builds it, at `0xbfb613a`–`0xbfb6188`:

```
s = NumberToString16(0x14)            // 0xbfb613a: mov esi, 0x14 → 20
label = GetStringFUTF16(0x19a1, s)    // 0xbfb6151
add("batterySaverModeEnabledBelowThresholdLabel", label)   // 0xbfb6176
```

So the threshold is **20 %**, inclusive ("inférieur ou égal").

Sanaga: Settings › Performance › Mode économie d'énergie gains "Batterie à
20 % ou moins" (`low_power = 3`): active when the computer runs on its
battery and the battery's level (`/sys/class/power_supply/*/capacity`, the
lowest one) is ≤ 20 (`src/app/tools.rs` `LOW_BATTERY_PERCENT`,
`low_power_active`). The other choices stay: off, on battery (default),
always. What the energy saver does in Sanaga (inactive tabs asleep after 5
minutes, slower interface refresh) is Sanaga's own: Chrome's lowers frame
rates and background work instead (not read here).

## Address bar — URL shown when not editing

Observed in Chrome:
- the address bar shortens addresses by default, and its right-click menu
  offers **"Toujours afficher les URL en entier"** (`fr.pak` 10536; menu
  item `OmniboxContextMenuMixinBase::kShowFullUrlsMenuItem`, text at
  `0x1cc305a`, used from the table at `0x14c11c8`);
- the path stays visible: the address follows every page, not only the
  site.

Inferred (not read in the code: Chrome's URL formatter has no symbols and
its flags are plain numbers): what the shortening leaves out — the
`https://` scheme, a leading `www.`, a lone `/` — and that the path is shown
decoded.

Sanaga (`src/app/toolbar.rs` `display_url`, used by `address_field`): the
site in full colour, the rest dimmed (`fr.wikipedia.org/wiki/Yaoundé`);
`https://`, `www.` and a lone `/` left out; `http://` kept (Sanaga shows a
padlock for https and nothing else for http). Settings › Apparence ›
"Toujours afficher les URL en entier" (`full_urls`, off by default) shows the
whole address. Before, Sanaga showed the site's name only, which didn't
change from page to page within a site.

## Also observed, not used yet
- Chrome keeps a list "Toujours laisser ces sites actifs" (sites never put to
  sleep), `fr.pak` 6548–6557.
- Inactive tabs show a dotted circle around their site icon (`fr.pak`
  6564–6565).
