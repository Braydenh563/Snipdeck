<#
  S2 automations. IME composition cannot be driven reliably with SendInput (the IME owns the composition
  and the candidate window), so every IME step stays manual. Automated: the CJK font-fallback rows (16a-c),
  checked against the font report the app prints at start-up; the visual comparison is one y/n question.
#>

function Get-S2CjkMode($Ctx) {
    $a = @()
    if ($Ctx.App -and $Ctx.App.norm) { $a = @($Ctx.App.norm.argList) }
    for ($i = 0; $i -lt $a.Count - 1; $i++) { if ($a[$i] -eq '--cjk') { return [string]$a[$i + 1] } }
    return 'ja'
}

function Invoke-S2FontRows($Ctx) {
    $mode = Get-S2CjkMode $Ctx
    $r = $null
    for ($i = 0; $i -lt 40; $i++) {
        $r = ConvertFrom-S2FontReport (Get-AutoAppLines $Ctx)
        if ($r.Override.Count -gt 0 -or $r.SystemMode) { break }
        Wait-Auto 200
    }
    $q = switch ($mode) {
        'zh' { "Look at the S2 window: do the Han characters in the 'default stack' row look the same as in the 'Microsoft YaHei UI' row?" }
        'system' { '' }
        default { "Look at the S2 window: do the Han characters in the 'default stack' row look the same as in the 'Yu Gothic UI' row (kana like Yu Gothic, Hangul like Malgun Gothic, the 'no glyph' row blank or boxes)?" }
    }
    $res = Resolve-AutoChecks (Get-S2FontChecks $r $mode) @() $q
    if ($r.Effective.ContainsKey('Hani')) { $res.evidence += "; effective Han fallback: $($r.Effective['Hani'])" }
    return $res
}
