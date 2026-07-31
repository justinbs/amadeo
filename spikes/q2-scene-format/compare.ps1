# Applies two realistic edits to each candidate scene file and reports the resulting diffs.
#
#   pwsh -File compare.ps1
#
# The point is to judge diff behaviour empirically rather than by assertion. Both edits are applied
# by script so the diff contains exactly the intended change and nothing else — hand-editing four
# files identically is exactly the kind of thing that quietly biases a comparison.
#
# Edit A: tune one number (a light's intensity). The single most common edit anyone makes.
# Edit B: add a sibling entity. The most common structural edit.
#
# Uses .NET file APIs rather than Get-Content/Set-Content: Windows PowerShell 5.1 reads UTF-8 as
# ANSI and writes back with a BOM, which would corrupt the files and the diff along with them.

$ErrorActionPreference = "Continue"
$root = $PSScriptRoot
$utf8NoBom = New-Object System.Text.UTF8Encoding $false

$formats = @(
    @{ Name = "RON";    File = "scene.ron" },
    @{ Name = "TOML";   File = "scene.toml" },
    @{ Name = "KDL";    File = "scene.kdl" },
    @{ Name = "custom"; File = "scene.scene" }
)

# --- Edit B: the block to insert, and the line to insert it before, per format ---

$insertBefore = @{
    "scene.ron"   = '                Entity('
    "scene.toml"  = '[[entity]]'
    "scene.kdl"   = '        entity id="a3"'
    "scene.scene" = '  entity a3 "Door"'
}

# Matches the *third* entity (the door) in RON and TOML, where the marker is not unique.
$occurrence = @{ "scene.ron" = 2; "scene.toml" = 3; "scene.kdl" = 1; "scene.scene" = 1 }

$newEntity = @{}

$newEntity["scene.ron"] = @'
                Entity(
                    id: "a5",
                    name: "CeilingLight2",
                    components: {
                        "PointLight": {
                            "color": [1.0, 0.85, 0.6],
                            "intensity": 2.0,
                            "range": 6.0,
                        },
                        "Transform2d": {
                            "position": [6.0, 2.5],
                            "rotation": 0.0,
                            "scale": [1.0, 1.0],
                        },
                    },
                    children: [],
                ),
'@

$newEntity["scene.toml"] = @'
[[entity]]
id = "a5"
name = "CeilingLight2"
parent = "a1"

[entity.components.Transform2d]
position = [6.0, 2.5]
rotation = 0.0
scale = [1.0, 1.0]

[entity.components.PointLight]
color = [1.0, 0.85, 0.6]
intensity = 2.0
range = 6.0

'@

$newEntity["scene.kdl"] = @'
        entity id="a5" name="CeilingLight2" {
            Transform2d {
                position 6.0 2.5
                rotation 0.0
                scale 1.0 1.0
            }
            PointLight {
                color 1.0 0.85 0.6
                intensity 2.0
                range 6.0
            }
        }

'@

$newEntity["scene.scene"] = @'
  entity a5 "CeilingLight2"
    Transform2d
      position 6.0 2.5
      rotation 0.0
      scale 1.0 1.0
    PointLight
      color 1.0 0.85 0.6
      intensity 2.0
      range 6.0

'@

# --- Build the edited copies ---

foreach ($directory in @("edit-a-tune-a-value", "edit-b-add-an-entity")) {
    $path = Join-Path $root $directory
    if (Test-Path $path) { Remove-Item $path -Recurse -Force }
    New-Item -ItemType Directory -Path $path | Out-Null
}

foreach ($format in $formats) {
    $file = $format.File
    $source = [System.IO.File]::ReadAllText((Join-Path $root "candidates\$file"))

    # Edit A: the light's intensity, 3.2 -> 4.5. One value, in one place, in every format.
    # The pattern has to cope with RON's `"intensity": 3.2`, TOML's `intensity = 3.2`, and the
    # bare `intensity 3.2` of KDL and the custom format.
    $tuned = $source -replace '("?intensity"?[:= ]+)3\.2', '${1}4.5'
    [System.IO.File]::WriteAllText((Join-Path $root "edit-a-tune-a-value\$file"), $tuned, $utf8NoBom)

    # Edit B: insert a sibling entity before the Nth occurrence of the marker line.
    $lines = $source -split "`n"
    $marker = $insertBefore[$file]
    $wanted = $occurrence[$file]
    $seen = 0
    $output = New-Object System.Collections.Generic.List[string]
    $inserted = $false
    foreach ($line in $lines) {
        # StartsWith, not equality: the KDL and custom markers are the beginning of a longer line.
        if (-not $inserted -and $line.TrimEnd("`r").StartsWith($marker)) {
            $seen++
            if ($seen -eq $wanted) {
                foreach ($new in ($newEntity[$file] -split "`n")) {
                    $output.Add($new.TrimEnd("`r"))
                }
                $inserted = $true
            }
        }
        $output.Add($line.TrimEnd("`r"))
    }
    if (-not $inserted) { Write-Output "WARNING: marker not found in $file" }
    [System.IO.File]::WriteAllText(
        (Join-Path $root "edit-b-add-an-entity\$file"),
        ($output -join "`n"),
        $utf8NoBom)
}

# --- Report ---

# Content lines only: no blanks, and no comments. Each candidate carries a header comment of a
# different length explaining its own design, and counting those would measure how much I wrote
# about a format rather than how compactly the format expresses the scene.
function Get-ContentLines {
    param([string]$Path)
    $lines = [System.IO.File]::ReadAllText($Path) -split "`n"
    ($lines | Where-Object {
        $trimmed = $_.Trim()
        $trimmed -ne "" -and -not $trimmed.StartsWith("#") -and -not $trimmed.StartsWith("//")
    }).Count
}

function Show-Diff {
    param([string]$Directory, [string]$Title)

    Write-Output ""
    Write-Output "=============================================================="
    Write-Output " $Title"
    Write-Output "=============================================================="
    Write-Output ("  {0,-8} {1,14} {2,9} {3,9}" -f "format", "content lines", "+ added", "- removed")

    foreach ($format in $formats) {
        $file = $format.File
        $before = Join-Path $root "candidates\$file"
        $after = Join-Path $root "$Directory\$file"

        $diff = git diff --no-index --numstat -- $before $after 2>$null
        $added = 0
        $removed = 0
        if ($diff) {
            $parts = ($diff -split "`t")
            $added = [int]$parts[0]
            $removed = [int]$parts[1]
        }
        Write-Output ("  {0,-8} {1,14} {2,9} {3,9}" -f `
            $format.Name, (Get-ContentLines $before), $added, $removed)
    }
}

Write-Output "Q2 scene format comparison"
Write-Output "Content lines exclude blanks and comments -- see Get-ContentLines for why."

Show-Diff -Directory "edit-a-tune-a-value" -Title "EDIT A - tune one value (light intensity 3.2 -> 4.5)"
Show-Diff -Directory "edit-b-add-an-entity" -Title "EDIT B - add a sibling entity (a second ceiling light)"

Write-Output ""
Write-Output "=============================================================="
Write-Output " MERGE - two authors, edit A and edit B, same scene, no coordination"
Write-Output "=============================================================="
Write-Output "  A compactness trap worth checking rather than assuming: the tighter a format is,"
Write-Output "  the closer two unrelated edits sit, and git's context windows can overlap where a"
Write-Output "  verbose format's would not."
Write-Output ""
Write-Output ("  {0,-8} {1}" -f "format", "three-way merge of A and B")

foreach ($format in $formats) {
    $file = $format.File
    $mine = Join-Path $root "edit-a-tune-a-value\$file"
    $base = Join-Path $root "candidates\$file"
    $theirs = Join-Path $root "edit-b-add-an-entity\$file"

    # git merge-file writes the result to <current> unless -p, so use -p and discard.
    $null = git merge-file -p --diff3 -- $mine $base $theirs 2>$null
    $conflicts = $LASTEXITCODE
    $verdict = if ($conflicts -eq 0) {
        "clean"
    } elseif ($conflicts -gt 0) {
        "$conflicts CONFLICT(S)"
    } else {
        "merge failed"
    }
    Write-Output ("  {0,-8} {1}" -f $format.Name, $verdict)
}

Write-Output ""
Write-Output "=============================================================="
Write-Output " Full diff for EDIT A, the most common edit there is"
Write-Output "=============================================================="
foreach ($format in $formats) {
    $file = $format.File
    Write-Output ""
    Write-Output ("--- " + $format.Name + " " + ("-" * 50))
    git diff --no-index --unified=1 -- `
        (Join-Path $root "candidates\$file") `
        (Join-Path $root "edit-a-tune-a-value\$file") 2>$null |
        Select-Object -Skip 4
}

# `git diff --no-index` exits 1 when files differ, which is the whole point here. Without this the
# script would look like it failed every time it succeeded.
exit 0
