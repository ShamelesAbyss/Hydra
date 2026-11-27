$ErrorActionPreference = 'Stop'
$hydraRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent

$violations = New-Object System.Collections.Generic.List[string]

# 1. Strict README.md compliance check (no direct artifact download links, shorteners, proxies, or redirects).
$readmePath = Join-Path $hydraRoot 'README.md'
$readme = [IO.File]::ReadAllText($readmePath)

$readmeForbidden = @(
    @{ Name = 'releases/download URL'; Pattern = '/releases/download/' },
    @{ Name = 'releases/latest/download URL'; Pattern = '/releases/latest/download/' },
    @{ Name = 'browser_download_url'; Pattern = 'browser_download_url' },
    @{ Name = 'URL shortener'; Pattern = 'https?://(bit\.ly|tinyurl\.com|t\.co|goo\.gl|shorturl\.at|is\.gd|cutt\.ly)/' },
    @{ Name = 'Direct binary/archive markdown link'; Pattern = '\]\(\s*https?://[^)\s]+\.(exe|apk|zip|tar\.gz|msi|dmg)\b' },
    @{ Name = 'Direct checksum markdown link'; Pattern = '\]\(\s*https?://[^)\s]+SHA256SUMS\.txt\b' },
    @{ Name = 'Direct binary/archive HTML href'; Pattern = 'href\s*=\s*["'']https?://[^"''\s]+(/releases/download/|\.(exe|apk|zip|tar\.gz|msi|dmg)|SHA256SUMS\.txt)' }
)

foreach ($rule in $readmeForbidden) {
    $m = [regex]::Matches($readme, $rule.Pattern, [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    foreach ($hit in $m) {
        $violations.Add("README.md prohibited $($rule.Name): $($hit.Value)")
    }
}

# 2. Verify every repository-relative image path in README.md exists with exact case match.
$assetDir = Join-Path $hydraRoot 'docs\assets'
$exactAssets = @{}
foreach ($f in (Get-ChildItem -Path $assetDir -File)) {
    $exactAssets[$f.Name] = $true
}

$imgMatches = [regex]::Matches($readme, '(?:src=["'']|\]\()(docs/assets/([^"''\)\s#?]+))')
foreach ($im in $imgMatches) {
    $rel = $im.Groups[1].Value
    $fname = $im.Groups[2].Value
    if (-not $exactAssets.ContainsKey($fname)) {
        $violations.Add("README.md references missing or wrong-cased asset: $rel")
    }
}

# 3. Scan all Markdown documents in the repository (outside target/ and .git/) for clickable prohibited links.
$docFiles = Get-ChildItem -Path $hydraRoot -Recurse -File -Include '*.md' | Where-Object {
    $_.FullName -notmatch '[\\/](target|\.git)[\\/]'
}

$docForbidden = @(
    @{ Name = 'Direct artifact download URL'; Pattern = 'https?://[^\s\)\"'']*(/releases/download/|/releases/latest/download/)' },
    @{ Name = 'Direct binary/archive/checksum link'; Pattern = '\]\(\s*https?://[^)\s]+(\.(exe|apk|zip|tar\.gz|msi|dmg)|SHA256SUMS\.txt)\b' },
    @{ Name = 'URL shortener'; Pattern = 'https?://(bit\.ly|tinyurl\.com|t\.co|goo\.gl|shorturl\.at|is\.gd|cutt\.ly)/' }
)

foreach ($doc in $docFiles) {
    $text = [IO.File]::ReadAllText($doc.FullName)
    $relPath = $doc.FullName.Substring($hydraRoot.Length + 1)
    foreach ($rule in $docForbidden) {
        $m = [regex]::Matches($text, $rule.Pattern, [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
        foreach ($hit in $m) {
            $violations.Add("$relPath prohibited $($rule.Name): $($hit.Value)")
        }
    }
}

if ($violations.Count -gt 0) {
    Write-Error ("GitHub distribution compliance check FAILED:`n - " + ($violations -join "`n - "))
    exit 1
}

Write-Output "GitHub distribution compliance verified across $($docFiles.Count) markdown files and $($imgMatches.Count) README asset references."
