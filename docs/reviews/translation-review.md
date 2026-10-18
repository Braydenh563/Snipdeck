# Translation review (owner or a native speaker)

All six catalogues (`crates/snipdeck-ui/lang/*/LC_MESSAGES/snipdeck-ui.po`) are complete:
`python3 scripts/i18n.py check` reports 0 untranslated. Machine-translated by the Haiku agent
(2026-10-18) with the SPEC §4 glossary; these choices want a native speaker's eye.

| msgid | Choice made | Alternative to consider |
|---|---|---|
| Escape | "Esc" everywhere | fr Windows: "Échap" |
| Control Enter | "Ctrl Enter" everywhere | de Windows: "Strg" |
| Insert (template submit, pastes into the target) | the paste verb: de Einfügen, es Pegar, fr Coller, pt-BR Colar, ja 貼り付け, zh 粘贴 | literal Insertar / Insérer |
| Insert {} (placeholder button) | de "{} einfügen", es "Insertar {}", fr "Insérer {}", pt-BR "Inserir {}", ja "{} を挿入", zh "插入 {}" | |
| Space (in "Hold Space to reveal") | translated: Leertaste, espaciadora, Espace, Espaço, スペースキー, 空格键 | |
| Fields (group label) | ja 入力欄, zh 字段 | zh 输入项 |
| Pinned (picker tag) | de Angeheftet, es Fijados, fr Épinglés, pt-BR Fixados, ja ピン留め, zh 置顶 | |
| Unpin | de "Lösen" | de "Nicht mehr anheften" |
| Your deck is empty | "deck" kept: de Dein Deck, fr Votre deck, ja デッキ, zh 卡组 | a plainer word |
| {n} item family | singular source, no plural forms (script has none yet) | add plurals when the script supports them |
| Tag / Tags | de Schlagwörter, es/pt-BR Etiquetas, fr Étiquettes, ja タグ, zh 标签 | |
| Use (card action) | de Verwenden, fr Utiliser, zh 使用 | de Einfügen in context |
| Sensitive item | de vertraulich, es/fr sensible, pt-BR sensível, ja 機密, zh 敏感 | |
| Theme / Light / Dark | de Design / Hell / Dunkel, ja ライト / ダーク, zh 浅色 / 深色 | |
| Teal / Slate / Rose | de Blaugrün / Schiefergrau / Rosé, zh 青绿色 / 石板灰 / 玫瑰色 | match DESIGN.md palette names |
| Enter (in "New tag, then Enter" etc.) | key name kept | fr Windows: "Entrée" |
| Long explanatory strings (Hidden in lists until you hold Space…, Fetches each site's small icon…, Rename or delete tags…, Choose one to add it…) | reworded freely | check length in the Picker and editor layouts on a run |
