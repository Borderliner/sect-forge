[CmdletBinding()]
param(
    [ValidateSet('patch', 'minor', 'major')]
    [string]$Part = 'patch',
    [switch]$CommitHook
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$manifestPattern = [regex]'(?m)^version\s*=\s*"(?<number>[^"]+)"'
$lockPattern = [regex]'(?m)^\[\[package\]\]\r?\nname = "sect-forge"\r?\nversion = "(?<number>[^"]+)"'

function Read-GitFile([string]$Object) {
    $lines = & git show $Object 2>$null
    if ($LASTEXITCODE -eq 0) { return ($lines -join "`n") }
}

function Read-Version([string]$Text, [regex]$Pattern, [string]$Label) {
    $number = $Pattern.Match($Text).Groups['number'].Value
    if ($number -cnotmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
        throw "Expected a MAJOR.MINOR.PATCH version in $Label."
    }
    return [version]$number
}

function Replace-Version([string]$Text, [regex]$Pattern, [string]$Version) {
    $number = $Pattern.Match($Text).Groups['number']
    return $Text.Remove($number.Index, $number.Length).Insert($number.Index, $Version)
}

Push-Location -LiteralPath $root
try {
    $previousManifest = Read-GitFile 'HEAD:Cargo.toml'
    $previous = if ($previousManifest) {
        Read-Version $previousManifest $manifestPattern 'HEAD:Cargo.toml'
    }

    if ($CommitHook) {
        $stagedManifest = Read-GitFile ':Cargo.toml'
        $stagedLock = Read-GitFile ':Cargo.lock'
        if (!$previous -and !$stagedManifest -and !$stagedLock) { return }
        if (!$stagedManifest -or !$stagedLock) {
            throw 'Stage both Cargo.toml and Cargo.lock for the versioned app.'
        }
        $stagedVersion = Read-Version $stagedManifest $manifestPattern 'staged Cargo.toml'
        $stagedLockVersion = Read-Version $stagedLock $lockPattern 'staged Cargo.lock'
        if ($stagedVersion -ne $stagedLockVersion) {
            throw 'Stage matching Sect Forge versions in Cargo.toml and Cargo.lock.'
        }
        # The first Rust commit sets the baseline. An explicit bump also
        # survives retries if Git stopped before writing the commit.
        if (!$previous -or $stagedVersion -gt $previous) { return }
        if ($stagedVersion -lt $previous) {
            throw "The staged version $stagedVersion is older than HEAD ($previous)."
        }
    }

    # git add would stage unrelated changes in a partially staged Cargo file.
    & git diff --quiet -- Cargo.toml Cargo.lock
    if ($LASTEXITCODE -ne 0) {
        throw 'Stage the intended Cargo.toml and Cargo.lock changes before bumping the version.'
    }

    $manifestPath = Join-Path $root 'Cargo.toml'
    $lockPath = Join-Path $root 'Cargo.lock'
    $manifest = [IO.File]::ReadAllText($manifestPath)
    $lock = [IO.File]::ReadAllText($lockPath)
    $current = Read-Version $manifest $manifestPattern 'Cargo.toml'
    if ($current -ne (Read-Version $lock $lockPattern 'Cargo.lock')) {
        throw 'Cargo.toml and Cargo.lock have different Sect Forge versions.'
    }
    $base = if ($previous) { $previous } else { $current }
    $next = switch ($Part) {
        'major' { "$($base.Major + 1).0.0" }
        'minor' { "$($base.Major).$($base.Minor + 1).0" }
        'patch' { "$($base.Major).$($base.Minor).$($base.Build + 1)" }
    }

    $newManifest = Replace-Version $manifest $manifestPattern $next
    $newLock = Replace-Version $lock $lockPattern $next
    try {
        [IO.File]::WriteAllText($manifestPath, $newManifest)
        [IO.File]::WriteAllText($lockPath, $newLock)
        & git add -- Cargo.toml Cargo.lock
        if ($LASTEXITCODE -ne 0) { throw 'Git could not stage the version bump.' }
    } catch {
        [IO.File]::WriteAllText($manifestPath, $manifest)
        [IO.File]::WriteAllText($lockPath, $lock)
        throw
    }
    Write-Host "Sect Forge $base -> $next"
} finally {
    Pop-Location
}
