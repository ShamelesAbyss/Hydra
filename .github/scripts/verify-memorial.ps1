$ErrorActionPreference = 'Stop'
$hydraRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$rule = [IO.File]::ReadAllText((Join-Path $hydraRoot '.cursor/rules/permanent-memorial.mdc')).Replace("`r`n", "`n")
$heading = '### In Loving Memory of Ashlan Haley Rapponotti'
$offset = $rule.IndexOf($heading, [StringComparison]::Ordinal)
if ($offset -lt 0) { throw 'Canonical memorial dedication is missing' }
$dedication = $rule.Substring($offset).TrimEnd([char[]]"`r`n") + "`n"
$readme = [IO.File]::ReadAllText((Join-Path $hydraRoot 'README.md')).Replace("`r`n", "`n")
if (-not $readme.EndsWith($dedication, [StringComparison]::Ordinal)) {
    throw 'The exact memorial dedication must be the final README section'
}
Write-Output 'Permanent memorial dedication preserved.'
