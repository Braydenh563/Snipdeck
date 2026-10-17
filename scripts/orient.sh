#!/usr/bin/env bash
# Start-of-session orientation, under 40 lines. bash, grep, sed, head only.
# Written by the lean-workflow plugin's lean-setup skill. Run it first.
cd "$(dirname "$0")/.." || exit 0
NOW=.claude/now.md
LEDGER=.claude/retro-ledger.md
echo "== now.md (head) =="
head -12 "$NOW" 2>/dev/null | cut -c1-200
echo "== Counts =="
section() { sed -n "/^## $1/,/^## /p" "$NOW" 2>/dev/null | grep -c '^[0-9-][0-9.)]* \|^- '; }
echo "open inbox items: $(section Inbox) (limit 20)"
echo "decisions recorded: $(section Decisions)"
echo "uncommitted paths: $(git status --short 2>/dev/null | wc -l)"
echo "== Remaining-work files (newest 5) =="
ls -t .claude/remaining/ 2>/dev/null | head -5
echo "== Code map =="
if [ -f scripts/codemap.py ]; then
  python3 scripts/codemap.py --check 2>&1 | head -1
  [ "${PIPESTATUS[0]}" = 0 ] && echo "fresh (grep docs/CODEMAP.md before opening files)"
fi
echo "== Last retro =="
tail -1 "$LEDGER" 2>/dev/null | cut -c1-200
exit 0
