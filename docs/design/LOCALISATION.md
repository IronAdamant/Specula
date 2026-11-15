# Design: localisation

Status: built in M5 (lane m5-l10n, 2026-11-07). ROADMAP: "string tables, plural rules, per-locale
fonts; agents translate and `ui_check` catches overflow". Code: `crates/specula-lingua` (the data),
`crates/specula-ui/src/strings.rs` and `state.rs` (the toolkit), `crates/specula/src/l10n_tools.rs`
(the tools), `games/sepulcra-app/src/l10n.rs` (the game). Proof: `scripts/l10n_check.py`.

**Goal:** every word a game shows lives in a string table, one file per language, so agents can
translate a game in parallel, review each other, and check every language in the interface at
every display before a human looks, and so changing the source words marks what needs
translating again.

## String tables

Project files `assets/strings/<locale>.json` (file kind `strings`, through the project store:
claims on `/project/assets/strings/<locale>.json`, versions, the journal, undo, disk, hot
reload). Format `specula.strings.v1`:

```text
{
  "format": "specula.strings.v1",
  "locale": "de",                       // BCP 47 subset: en, de, pt-BR, zh-Hant-TW, es-419
  "name": "Deutsch",                    // the language's own name (menus)
  "source": true,                       // the source table only (English)
  "status": "unreviewed",               // the default of plain entries: unreviewed | reviewed
  "number": {"decimal": ",", "group": ".", "min_group": 1, "percent": "# %"},  // optional
  "plural": {"one": "i = 1 and v = 0"}, // optional: CLDR rules, replacing the built-in ones
  "fonts": {"ui": "system:ja"},         // optional: a face per font slot in this locale
  "strings": {
    "hud.wave": "Welle {wave}/{waves}",
    "toast.tombstones_lost": {"plural": "remaining", "one": "... {remaining} Grabstein", "other": "... {remaining} Grabsteine"},
    "hud.menu": {"text": "Menü", "status": "reviewed", "src": "9c1f04a2b37de810", "notes": "the pause button"}
  }
}
```

- **Keys** are 1..96 of letters, digits, `_ - .`, starting with a letter; dots namespace them
  (`hud.wave`, `stone.warding.name`, `dialogue.intro.1`).
- **Texts** use the UI's template syntax: `{name}`, `{name:.N}`, `{name:time}`, `{name:pct}`,
  `{name:plain}` (digits as they are: seeds, years), `{{` `}}` for braces. A text is at most 2048
  bytes and 16 placeholders.
- **Plural forms** are picked by one placeholder's number as it is shown (CLDR: `1` is "one" in
  English, `1.0` is "other"). `other` is required.
- **`src`** is the stable hash of the source words a translation was made from. When the source
  changes, the translation is **stale** (`l10n_check`, `l10n_get {status: "stale"}`), and a
  reviewer can't approve it until it is translated again.
- **The fallback chain** is the locale, each parent (`de-AT`, `de`), then the source table. A
  parent is the shorter prefix, except where CLDR's parent locales say otherwise
  (`Locale::parent`): a script that isn't its language's usual one never reads the bare language
  (`zh-Hant-TW` reads `zh-Hant` and then the source, never `zh`, which is Simplified; so do
  `zh-TW`, `zh-HK` and `zh-MO`, which write Traditional; `sr-Latn` never reads Cyrillic `sr`), Latin American Spanish reads `es-419` before `es`, and European
  and African Portuguese `pt-PT` before `pt`. A key missing in a translation shows the source's
  words; a key missing everywhere shows as `@key`, so it is seen (and `ui_check`'s `missing_key`
  names it).
- **Hidden characters are refused** in texts, plural forms, the language's name and notes: the
  bidirectional embeddings, overrides and isolates, the Unicode tag characters (invisible copies
  of ASCII: the usual way to hide instructions for an agent reading the table), the invisible
  operators, the byte-order mark and the line and paragraph separators (`table::hidden`). They
  show nothing and can make words read in another order than they are stored, and a reviewer
  must be able to see what a translation says. What real text is typed with is allowed: the
  zero-width space (the word break of Thai, Lao, Khmer and Burmese), the joiners and direction
  marks of Arabic, Persian, Hebrew and Indic text, the soft hyphen, emoji's variation selectors.
  Notes take no control characters but line breaks.
- **The pseudo-locale** `qps-ploc` is made from the source, never written: letters accented, about
  35% longer, in brackets, placeholders kept. Overflow shows before anyone translates.

`specula-lingua` is a simulation crate (determinism lints; integer arithmetic, `std`'s exact
number formatting and `dmath` only): game logic and modules may format text from it, and a replay
reads the same words.

### Plural and number rules as data

- Built in: CLDR 44's cardinal rules for 70 languages (`plural::BUILTIN`: English, German, Dutch,
  the Scandinavian languages, French, Spanish, Italian, Portuguese (pt and pt-PT), Russian,
  Ukrainian, Polish, Czech, Romanian, Hebrew, Arabic, Japanese, Chinese, Korean, and since the
  lead's review Lithuanian, Latvian, Slovenian, Croatian, Serbian, Bosnian, Macedonian,
  Icelandic, Irish, Welsh, Maltese, Hindi, Bengali, Persian and others), each tested against
  CLDR's samples. **A language with no rules is reported**, never silently "other only": a table
  for a language that isn't built in and gives no `plural` of its own gets the `no_plural_rules`
  warning from `l10n_check`, and `l10n_get` says where a locale's rules come from
  (`plural_rules: "builtin" | "table" | "none"`). A table's `plural` replaces the built-in rules
  with its own in CLDR syntax
  (operands `n i v w f t e c`, `%`/`mod`, `=`, `!=`, ranges, `and`, `or`; samples after `@`
  ignored), parsed with structured errors (byte offsets).
- Operands come from the number's shown digits (its format decides `v`, `f`, `t`), integers up to
  18 digits exactly (longer ones keep their last 18 digits for modulus and equal no small value).
- Number formats per locale: decimal and grouping separators, CLDR's minimum grouping digits
  (Spanish and Polish leave `1234` ungrouped) and the percent pattern; a table's `number`
  overrides them. French groups with a no-break space (U+202F is missing from many faces).
- Without a string table a UI prints numbers as before tables (plain ASCII, no grouping), so a
  game that isn't localised doesn't change.

## In the toolkit (`specula-ui`)

- **Screens** show an entry with `"text": "@hud.wave"` (any template field: label, button text,
  tooltip, radial label and detail); `"@@x"` is a literal `@x`. The entry's placeholders read the
  screen's state, as the screen's own template would.
- **Game words in state**: `StateMap::set_msg(key, "stone.tier_name")` returns the message's own
  arguments to fill (`.set_int("n", 3)`, nested messages with `set_msg` again). Storage is reused
  frame to frame (no allocation once warm, F3). Messages nest at most 4 deep and every text is
  capped at `MAX_TEXT` bytes as it is written, so hostile nesting costs at most the cap.
  `Value::Str` stays for words that are no table's (a save's own name, a key's glyph); in the
  pseudo-locale `ui_check`'s `untranslated` names them.
- **`Ui::set_strings(Some(Arc<Strings>))`** switches every screen's language at once; the locale's
  faces take their font slots (`FontSet` keeps the base faces; the atlas keys glyphs by face, so
  nothing stale is drawn). `Strings::load_fonts` reads the faces a table names (bundled by name,
  `system:<locale>` from the system's CJK faces for that locale, 10-20 MB each;
  `specula_text::load_cjk_face`) and must run off the engine thread; faces are cached per
  process (at most 6). On a machine with no such face the slot is a problem that is said
  ("no system face for ja on this machine"), never another face in its place.
- **`ui_check`** gains `missing_key` (error: a `@key` the table lacks), `hardcoded` (warning, with a
  table: words a screen writes itself) and `CheckOpts::strings` (check in another language; the
  player's table is put back). `pseudo: true` with a table swaps in the pseudo-locale made from
  its source. `ui_tree` gives each `@key` label's `text_key`.
- **Nothing here builds strings on the engine thread** (AGENTS.md rule 15; the lead's review
  measured 44-128 ms stalls with a table at the format's size). A `Strings` makes its
  pseudo-locale once and keeps it (`Strings::pseudo`), and whoever builds a `Strings` makes it
  there (`warm_pseudo`): the tools on the caller's connection, the game where it reads its
  tables. Sepulcra builds every table's strings when the tables are read (at start, or on its
  reload thread), so the Language button takes a reference; a locale with no table of its own
  shows its nearest table at once and gets its own strings from a thread.
- **Messages nest `MAX_DEPTH` (4) deep** in a screen, the same as in the text the game renders
  itself. A test renders what the game says (the HUD's hover, the saves screen's dates, refusals)
  both ways in every language and requires the same words.

## Tools

| Tool | Access | What |
|---|---|---|
| `l10n_get` | read | the tables and their counts (translated, missing, unreviewed, reviewed, stale) and the game's language; or one locale's entries resolved through its chain, filtered (`keys`, `prefix`, `status`) and paged, with `preview`s of plural forms and numbers (`args`) |
| `l10n_set` | world:project | writes, removes or reviews entries of one table in one commit (through `project_write`) |
| `l10n_check` | read | the tables against each other, the screens and timelines (`@key`s, written words) and the game's keys; with `ui`, the game's interface in each locale (and the pseudo-locale) at every target and scale asked, per screen group |
| `l10n_tasks` | owned:board | the translation flow: one plan with a translate task (role translator) and a review task after it (role reviewer) per locale with work |
| `ui_check {locale}` | read | `ui_check` in another language |

**Writes.** `l10n_set` writes tables in one form (`l10n_tools::render_table`: one header field
per line, one string per line, so a diff is one line per string; the committed tables are in that
form) and refuses before writing: a key the source lacks (`unknown_key`), a translation whose
placeholders aren't the source's (`placeholders`, naming what is missing and extra), another
plural count (`plural_count`), `status` or `src` given by the caller (they are the tool's).
Translations are stored `unreviewed` with `src`. A table that doesn't exist needs
`create: {name}`. **Cost:** at most 200 edits per call (counted before anything is copied), one
parse and one print of the table (at most 1 MiB) whatever their number; the result goes through
`project_write`'s size rule.

**Reviews.** Marking entries reviewed needs reviewer authority (`authority::Site::L10nReview`,
code `not_reviewer`), both through `l10n_set {review}` and when a table is written with
`project_write`/`project_patch` (`l10n_tools::review_gate`: an entry reviewed in the new text must
have been reviewed in the old one with the same words and source, **in the same language**: a
table whose `locale` changes carries none of its reviews over, so nobody relabels a reviewed
German table as French). **A string table that doesn't parse is never saved**, `allow_errors` or
not (`project_kinds::never_allowed`): a broken table shows the game no words and would hide every
review it held from this gate. `src` is written only by
`l10n_set` (which works out the source's hash itself): adding, changing or dropping one by hand
needs reviewer authority too, so nobody hides a stale translation. A stale entry, or one with no
`src`, can't be reviewed; one call never writes, removes or reviews one key twice. Edits from
disk (a human's editor) are imported as `disk`, as for every project file. Owner-decided taste
results (`gallery_apply`, `taste_suggest apply`) write through `project_put_all` without the gate:
the owner's decision is the review.

**Limits of `l10n_check`.** At most 2,048 layouts per call (locales × groups × targets × scales,
refused up front with `limit`), 8 MiB of screen and timeline files scanned for keys (past it a
`scan_limit` warning), findings quoted per locale so the answer stays under its ceiling.

**Threads.** Everything runs on the calling agent's connection: the engine lends the store's
texts (`ProjectCall::OfKind`, Arcs, no copies), commits, and lays the interface out
(`ui_check`'s step job, one display per step). Parsed tables are cached by path and version.

**`ui_check {locale}`** inside a playtest is refused (`locale_unavailable`): its tables are read
on the caller's connection, which a playtest step on the engine thread can't wait for;
`pseudo: true` works there.

## The game (Sepulcra)

- Every word a player sees comes from `games/sepulcra/assets/strings/en.json` (281 keys: the
  screens' texts, the HUD's state, toasts, confirmations, the saves screen, controls, threat
  arrows, the cursor's hover line, refusals, subtitles). `en.json` is also built into the game.
  German (`de.json`) was translated by an agent through `l10n_set` and is unreviewed.
- The game's code says things with `l10n::Said` (a key and arguments, nested messages included);
  the HUD gets them as messages (`l10n::put`), toasts are written in the player's language, and
  the agents' log (`play_state`) and `play_command`'s refusals stay in the source language, so
  agents read the same words whatever the player chose.
- The language: `-- --locale de` at start, or the settings screen's **Language** button (cycles
  the tables there are). A table edited on disk or with `l10n_set` hot-reloads into the HUD.
- `Game::locale` tells the engine the game's language and every key its code may show (fixed
  ones plus one per stone, enemy, prop and input action), so `l10n_check` finds keys the code
  uses that the source lacks, and keys nothing uses.
- Subtitles in timelines are keys (`"text": "@subtitle.victory"`, `"speaker": "@speaker.warden"`);
  text without `@` is shown as written.
- **Not localised, on purpose:** the epitaphs and the title stone are world text, diegetic art
  (Latin, Russian and Greek by design), not interface; key glyphs (`Space`, `LMB`) come from
  `specula-input`'s binding names (their localisation is the input crate's, later); the window
  title is the game's name.

## Not built yet

- Complex shaping (Arabic, Hebrew, Indic) is the text lane's T4 design note: tables can hold those
  languages, but the bundled faces and layout don't draw them correctly yet.
- Gendered and ordinal forms (CLDR ordinals, select on gender) and nested plural arguments in one
  text: one count per entry today.
- Translation memory and glossary checks.
