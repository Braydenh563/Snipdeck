Snipdeck project site (static, no build step, no JavaScript, no external requests)

Publish: GitHub Pages, through a Pages workflow that uploads site/ as the artifact (Pages itself only serves / or /docs). Preview: cd site && python3 -m http.server

The Supporter address is in 2 mailto links in index.html; keep it the same as SUPPORTER_EMAIL in crates/snipdeck-app/src/settings_binding.rs.
Free vs Supporter table: one <tr> per feature in #compare. Check it against crates/snipdeck-core/src/entitlements.rs.

Files
  index.html   the page; a Content-Security-Policy meta tag blocks every request to another site
  style.css    DESIGN.md / design/tokens/palette.json tokens as custom properties, light and dark
  fonts/       IBM Plex Sans 400/500/600 and Mono 400/500, Latin subset, woff2, SIL OFL (fonts/OFL.txt)
  img/         screenshots copied from docs/images/ (hero, panel, picker, fill, main; light and dark)
  favicon.svg  assets/logo/snipdeck-icon.svg

Text colour pairs (WCAG 2.2 AA needs 4.5:1; measured from the rendered page)
  Light: text.primary #171613 on bg.app #F5F3F1 16.35, on bg.surface #FBFAF7 17.34
         text.secondary #5D5A57 on bg.app 6.19, on bg.surface 6.56, on bg.sunken #EBE9E7 5.66
         accent.text #195AB3 on bg.app 6.01, on bg.surface 6.38, on bg.selected #EBF2FE 5.92
         text.on-ink #FBFAF7 on ink #171613 17.34
  Dark:  text.primary #EBE9E7 on bg.app #110F0D 15.79, on bg.surface #171613 14.94
         text.secondary #A09E9A on bg.app 7.15, on bg.surface 6.77, on #201E1B 6.22
         accent.text #ABCDFE on bg.app 11.74, on bg.surface 11.11, on bg.selected #05306A 7.88
         text.on-ink #171613 on ink #EBE9E7 14.94
Non-text (3:1): focus ring #2870D4 on #F5F3F1 4.35, #74ACFF on #110F0D 8.28; tick #007132 5.9 / #A0DCAC 11.48;
  secondary-button border #7C7A76 on surface 4.1 (light) / 4.22 (dark).
