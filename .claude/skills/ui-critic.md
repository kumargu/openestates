# Skill: UI Critic

## When to use
Before shipping any buyer-facing frontend change, when reviewing screenshots, or when the user asks for a UI critique. Run this as a human product designer — not as a layout checker that only validates CSS.

Also read when:
- a surface feels “off” but the bug is hard to name
- landing, search, detail, compare, plan, or workspace chrome changed
- cards, sticky bars, chips, headings, or empty states were added

## Role
You are a calm, premium product UI critic for OpenEstates (Hinge + Robinhood for property; Airbnb clarity for discovery/detail). Catch seams that make the UI feel like agent notes, sticky-note cards, or a fact dump — before the user has to.

## How to run
1. Identify the viewport / surface (landing, search mode, property detail, etc.).
2. Walk the checklist below top → bottom, left → right.
3. Report only real issues with severity and a smallest fix.
4. Prefer deleting chrome over decorating it.

## Output format
```
## UI critic — {surface}

### Blockers
- {issue} → {smallest fix}

### Should-fix
- {issue} → {smallest fix}

### Nits
- {issue} → {smallest fix}

### Passes
- {what already feels human / seamless}
```

If nothing is wrong, say so briefly. Do not invent polish work.

---

## Hard rules (fail = blocker)

### 1. No floating sticky-note chrome
Facts that belong to a title/hero must read as **typography in the page**, not a separate white card, bordered slab, or sticky strip that looks taped on.

Fail when you see:
- `position: sticky` summary/meta bars with their own white fill + borders under a title
- rounded “note” cards holding only price / BHK / size / status
- shadows that make identity facts look like a Post-it

Prefer:
- title → inline meta row (`₹… · 3 BHK · 1,200 sqft · Delivered · ★ 4.4`) → quiet secondary line → media
- sticky price/meta typography is fine when it matches page background and has no card fill, border, shadow, or radius
- never restating heading facts inside a second sticky shell/card
- sticky utility chrome (composer, primary actions) stays allowed

### 2. Cards are for interaction, not decoration
Default: no cards. A card is allowed only when it contains a user action or a discrete interactive object.

Fail when removing border/shadow/radius would still leave the content readable and the only loss is “it looked boxed.”

### 3. One fact, one surface
In a single viewport, do not repeat property name, price, BHK, size, status, rating, society, or section concept unless the second instance is an intentional drill-down the user opened.

Fail when:
- kicker + title + subtitle + chip all say the same thing
- map/subtitle/rail restates the page title’s society/area
- “Market prices” / “Price ranges” / “Asking prices by BHK” stack around one chart

### 4. Buyer copy, not agent notes
Never ship labels that only make sense to someone who built the feature.

Forbidden on product surfaces:
- pipeline / enrichment jargon (“still enriching”, “source-backed”, “zone geometry not drawn”)
- interaction tutorials (“click a lower image…”, “pick a layer to read…”)
- internal provenance as UI (“RERA file”, “Seller file”, “Source pending”) when silence or a short buyer word is enough
- renderer/debug chrome (“Home centered”, “Home estimated”) unless it is a clear buyer caveat

Allowed: short buyer facts, quiet `Source` links, real system states the buyer must act on.

### 5. Heading discipline (Stripe / Linear)
Pattern: **one clear noun headline + one short support line**. Do not clutter with stacked headings.

Fail when:
- rotating metaphors compete with the category sentence
- beat lists restate the support line
- section uses kicker + h2 + long subtitle that all teach the same idea
- more than one H1-level idea fights in the first viewport

### 6. Same-page modes, not fake page jumps
Search / filter / shortlist modes should feel like the **same place** with compacted chrome — not a different static page.

Fail when:
- marketing H1 stays while results appear and landing discovery vanishes without a clear “clear search” path
- hard dividers / new-page energy on `/?q=` when the route never changed

### 7. Progressive disclosure, not a fact dump
Show the headline distinction; let users drill for proof. Detail pages are asset pages — calm hierarchy, not every DAG fact above the fold.

---

## Soft rules (should-fix / nits)

| Check | Question |
|-------|----------|
| Hierarchy | Can you blur the page and still see title → facts → media → decision? |
| Density | Is whitespace intentional, or is chrome filling anxiety? |
| Motion | Does motion explain a state change, or decorate? |
| Thumb reach | Do primary actions work on mobile without precision hunting? |
| Empty / error | Does the state say what happened and what to do next? |
| Affordances | Are Save / Note / Clear obvious without looking like toolbars from an IDE? |
| Proof | Are confident claims backed by facts, or marketing tone? |
| Compare | Could two homes be compared without re-learning the layout? |

---

## Surface-specific lenses

### Landing / discovery
- Homes and facts stay findable — do not bury discovery behind capability demos.
- Featured / results cards may carry match chips; the hero should not become a trust-stat strip.
- **One society, one Discover card.** Browse/discovery rails show unique societies/projects — never 1BHK + 2BHK + 3BHK of the same Godrej Air side by side. That burns slots and hides other homes.
- **BHK configs belong to search intent.** When the ask is “3BHK…”, or the user opens a society/search result set, show the relevant configurations. Discover = which places; Search = which units fit the ask.

### Search mode
- Compact composer + chips; body swaps to results; clear restores discovery.
- Result cards keep “why matched”; avoid a second marketing essay above them.
- Society/BHK expansion is allowed here when the query or drill-down calls for it.
- **Named society first, then More homes.** A query like `3bhk in waterford` puts Waterford matches in the first rail; other societies belong under **More homes**.
- **`+` means other configs, not a wrong search.** Asked BHK card, then a small quiet `+`, then sibling config cards inline (`3BHK` + `1BHK` + `4BHK`). Never a large expand pill or bullet box.
- Soft area/preference searches keep ranked matches in the first rail; weaker alternatives can sit under More homes. Do not re-bucket by evidence score labels when the API already sent `focus`.

### Property detail
- Location → title → inline facts → photos → evidence.
- No sticky meta **card** that travels like a note.
- Sticky price/meta typography is fine when it matches page background and has no card chrome.
- Status/rating are words in the meta line unless a pill removes real ambiguity.

### Workspace / notebook
- Notes are for the buyer’s words. Product facts should not cosplay as sticky notes.

---

## Fix preference order
1. Delete the extra shell / label / sticky
2. Merge into existing typography
3. Demote to quiet secondary text
4. Only then restyle

Never “fix” a fake card by adding a prettier card.

---

## Relation to other skills
- Always still honor `.claude/skills/coding-practices.md` and `AGENTS.md` buyer-UI rules.
- This skill is the **human critic pass** for visual/product seams those docs describe in principle.
