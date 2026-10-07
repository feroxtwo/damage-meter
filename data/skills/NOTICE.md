# AION 2 artwork and catalogue attribution

Skill and class artwork depicts AION 2 and remains third-party game artwork owned by NCSOFT and its respective rights holders. The meter's software licence does not grant ownership of or a separate licence to this artwork. This project is not affiliated with NCSOFT, Wakayashi or Questlog.

Sources observed for this integration:

- https://wakayashi.gg/aion2 — class/skill identities and skill artwork.
- https://questlog.gg/aion-2/de/db/skills — German catalogue and ID-based artwork.
- https://questlog.gg/aion-2/en-nc/db/skills — English NCSOFT names, including the Brawler.

`catalog.json` records original URLs per skill. `icons.json` records the selected artwork URL, byte offset, length and SHA-256 for every bundled WebP. Wakayashi relative URLs resolve against `https://wakayashi.gg`. Images are scaled to at most 64 × 64 and encoded as WebP at quality 70. The nine native class images are 32 × 32 RGBA, in the order recorded by `catalog.json`.

The package contains 353 skill symbols and nine class symbols. 122 skill entries use artwork from Wakayashi; the other skill symbols use Questlog artwork. Eleven general catalogue entries expose no artwork in the source and use a visible unknown-symbol fallback. Shared source artwork is deduplicated in `icons.bin`.

The 41 German Brawler names are community translations maintained by this project and explicitly marked `community`. Questlog's German catalogue did not list those skills when collected. No guide descriptions, build recommendations or commercial site branding were copied. The sources and translations are a snapshot, not a promise of completeness for future game patches.
