$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$sourceExtensions = @(
    '.rs', '.js', '.mjs', '.cjs', '.ts', '.tsx', '.jsx',
    '.css', '.scss', '.html', '.htm', '.py', '.sh', '.ps1', '.bat',
    '.sql', '.lua', '.go', '.java', '.c', '.h', '.cpp', '.hpp', '.cs', '.php',
    '.vue', '.svelte'
)
$blockCommentExtensions = @('.rs', '.js', '.mjs', '.cjs', '.ts', '.tsx', '.jsx', '.css', '.scss', '.sql', '.go', '.java', '.c', '.h', '.cpp', '.hpp', '.cs', '.php', '.vue', '.svelte')

function Test-SourceLine {
    param(
        [string] $Line,
        [string] $Extension,
        [ref] $InBlockComment
    )

    if ([string]::IsNullOrWhiteSpace($Line)) { return $false }

    if ($Extension -in $blockCommentExtensions) {
        $remaining = $Line
        while ($remaining.Length -gt 0) {
            if ($InBlockComment.Value) {
                $close = $remaining.IndexOf('*/', [StringComparison]::Ordinal)
                if ($close -lt 0) { return $false }
                $remaining = $remaining.Substring($close + 2)
                $InBlockComment.Value = $false
                continue
            }

            $trimmed = $remaining.TrimStart()
            $lineComment = if ($Extension -eq '.sql') { '--' } else { '//' }
            if ($trimmed.Length -eq 0 -or $trimmed.StartsWith($lineComment)) { return $false }
            $open = $remaining.IndexOf('/*', [StringComparison]::Ordinal)
            if ($open -lt 0) { return $true }
            if (-not [string]::IsNullOrWhiteSpace($remaining.Substring(0, $open))) { return $true }
            $remaining = $remaining.Substring($open + 2)
            $close = $remaining.IndexOf('*/', [StringComparison]::Ordinal)
            if ($close -lt 0) {
                $InBlockComment.Value = $true
                return $false
            }
            $remaining = $remaining.Substring($close + 2)
        }
        return $false
    }

    if ($Extension -in @('.html', '.htm')) {
        $remaining = $Line
        if ($InBlockComment.Value) {
            $close = $remaining.IndexOf('-->', [StringComparison]::Ordinal)
            if ($close -lt 0) { return $false }
            $remaining = $remaining.Substring($close + 3)
            $InBlockComment.Value = $false
        }
        $open = $remaining.IndexOf('<!--', [StringComparison]::Ordinal)
        if ($open -lt 0) { return -not [string]::IsNullOrWhiteSpace($remaining) }
        if (-not [string]::IsNullOrWhiteSpace($remaining.Substring(0, $open))) { return $true }
        $close = $remaining.IndexOf('-->', $open + 4, [StringComparison]::Ordinal)
        if ($close -lt 0) {
            $InBlockComment.Value = $true
            return $false
        }
        return -not [string]::IsNullOrWhiteSpace($remaining.Substring($close + 3))
    }

    if ($Extension -eq '.ps1') {
        $remaining = $Line
        if ($InBlockComment.Value) {
            $close = $remaining.IndexOf('#>', [StringComparison]::Ordinal)
            if ($close -lt 0) { return $false }
            $remaining = $remaining.Substring($close + 2)
            $InBlockComment.Value = $false
        }
        if ($remaining.TrimStart().StartsWith('#')) { return $false }
        $open = $remaining.IndexOf('<#', [StringComparison]::Ordinal)
        if ($open -lt 0) { return -not [string]::IsNullOrWhiteSpace($remaining) }
        if (-not [string]::IsNullOrWhiteSpace($remaining.Substring(0, $open))) { return $true }
        $close = $remaining.IndexOf('#>', $open + 2, [StringComparison]::Ordinal)
        if ($close -lt 0) {
            $InBlockComment.Value = $true
            return $false
        }
        return -not [string]::IsNullOrWhiteSpace($remaining.Substring($close + 2))
    }
    if ($Extension -in @('.py', '.sh')) {
        return -not $Line.TrimStart().StartsWith('#')
    }
    if ($Extension -eq '.bat') {
        $trimmed = $Line.TrimStart()
        return -not ($trimmed.StartsWith('::') -or $trimmed -match '^(?i:@?rem)(\s|$)')
    }

    $trimmedLine = $Line.TrimStart()
    return -not ($trimmedLine.StartsWith('//') -or $trimmedLine.StartsWith('#'))
}

$trackedFiles = & git -C $repoRoot ls-files --cached --others --exclude-standard
if ($LASTEXITCODE -ne 0) { throw 'Could not enumerate repository source files with git.' }

$violations = [System.Collections.Generic.List[string]]::new()
$checkedFiles = 0
$largestFile = ''
$largestLineCount = 0
foreach ($relativePath in $trackedFiles) {
    $extension = [System.IO.Path]::GetExtension($relativePath).ToLowerInvariant()
    if ($extension -notin $sourceExtensions) { continue }

    $absolutePath = Join-Path $repoRoot $relativePath
    if (-not (Test-Path -LiteralPath $absolutePath -PathType Leaf)) { continue }

    $checkedFiles++
    $lineCount = 0
    $inBlockComment = $false
    foreach ($line in [System.IO.File]::ReadLines($absolutePath)) {
        if (Test-SourceLine -Line $line -Extension $extension -InBlockComment ([ref]$inBlockComment)) {
            $lineCount++
        }
    }
    if ($lineCount -gt $largestLineCount) {
        $largestFile = $relativePath
        $largestLineCount = $lineCount
    }
    if ($lineCount -gt 1000) {
        $violations.Add("${relativePath}: $lineCount lines of code (limit 1000)")
    }
}

if ($violations.Count -gt 0) {
    Write-Error ("Source line limit exceeded:`n - " + ($violations -join "`n - "))
    exit 1
}

Write-Output "Source line limit passed for $checkedFiles files (largest: $largestFile at $largestLineCount lines; limit 1,000)."
