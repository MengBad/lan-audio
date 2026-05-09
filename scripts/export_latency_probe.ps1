param(
    [string[]]$SnapshotPath = @(),
    [string]$OutputPath = "artifacts/latency/latency_probe_latest.json"
)

$ErrorActionPreference = "Stop"

function Get-NowIso {
    return (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
}

function Get-JsonFiles {
    param([string[]]$Paths)

    $files = New-Object System.Collections.Generic.List[string]
    foreach ($path in $Paths) {
        if ([string]::IsNullOrWhiteSpace($path)) {
            continue
        }
        $matches = Get-ChildItem -Path $path -File -ErrorAction SilentlyContinue
        foreach ($match in $matches) {
            if ($match.Extension -ieq ".json") {
                $files.Add($match.FullName)
            }
        }
    }
    return $files
}

function Get-FirstProperty {
    param(
        [object]$Object,
        [string[]]$Names
    )

    if ($null -eq $Object) {
        return $null
    }

    foreach ($name in $Names) {
        if ($Object.PSObject.Properties.Name -contains $name) {
            return $Object.$name
        }
    }
    return $null
}

function Convert-ToInt {
    param([object]$Value)

    if ($null -eq $Value) {
        return 0
    }
    try {
        return [int][double]$Value
    } catch {
        return 0
    }
}

function Get-ServiceSnapshot {
    param([object]$Document)

    if ($null -eq $Document) {
        return $null
    }

    if ($Document.PSObject.Properties.Name -contains "service_snapshot") {
        return $Document.service_snapshot
    }
    if ($Document.PSObject.Properties.Name -contains "snapshot") {
        $snapshot = $Document.snapshot
        if ($snapshot.PSObject.Properties.Name -contains "service_snapshot") {
            return $snapshot.service_snapshot
        }
        return $snapshot
    }
    if (($Document.PSObject.Properties.Name -contains "mode") -and
        ($Document.PSObject.Properties.Name -contains "metrics")) {
        return $Document
    }

    return $null
}

function New-Sample {
    param(
        [string]$SourceFile,
        [object]$ServiceSnapshot
    )

    $metrics = Get-FirstProperty $ServiceSnapshot @("metrics")
    $buffered = Convert-ToInt (Get-FirstProperty $metrics @("buffered_ms", "bufferMs"))
    $rtt = Convert-ToInt (Get-FirstProperty $metrics @("rtt_ms", "rttMs"))
    $sinkGap = Convert-ToInt (Get-FirstProperty $metrics @("sink_write_gap_ms_p95", "sinkWriteGapMsP95"))
    $latencyProxy = $buffered + $rtt + $sinkGap

    return [pscustomobject]@{
        source_file = $SourceFile
        mode = [string](Get-FirstProperty $ServiceSnapshot @("mode", "audio_mode", "current_audio_mode"))
        transport = [string](Get-FirstProperty $ServiceSnapshot @("transport", "transport_type"))
        data_plane = [string](Get-FirstProperty $ServiceSnapshot @("active_data_plane", "data_plane"))
        codec = [string](Get-FirstProperty $ServiceSnapshot @("effective_codec", "codec"))
        state = [string](Get-FirstProperty $ServiceSnapshot @("state", "playback_state"))
        buffered_ms = $buffered
        rtt_ms = $rtt
        sink_write_gap_ms_p95 = $sinkGap
        latency_proxy_ms = $latencyProxy
    }
}

function Get-Percentile {
    param(
        [int[]]$Values,
        [double]$Percentile
    )

    if ($Values.Count -eq 0) {
        return $null
    }
    $sorted = @($Values | Sort-Object)
    $index = [Math]::Ceiling(($Percentile / 100.0) * $sorted.Count) - 1
    if ($index -lt 0) {
        $index = 0
    }
    if ($index -ge $sorted.Count) {
        $index = $sorted.Count - 1
    }
    return $sorted[$index]
}

function New-ModeResult {
    param(
        [string]$Mode,
        [int]$TargetMs,
        [object[]]$Samples
    )

    $modeSamples = @($Samples | Where-Object { $_.mode -eq $Mode })
    if ($modeSamples.Count -eq 0) {
        return [pscustomobject]@{
            mode = $Mode
            target_latency_proxy_ms = $TargetMs
            sample_count = 0
            passed = $false
            status = "pending_no_snapshot"
            average_latency_proxy_ms = $null
            p95_latency_proxy_ms = $null
            max_latency_proxy_ms = $null
        }
    }

    $values = @($modeSamples | ForEach-Object { [int]$_.latency_proxy_ms })
    $average = [Math]::Round((($values | Measure-Object -Average).Average), 2)
    $p95 = Get-Percentile -Values $values -Percentile 95
    $max = ($values | Measure-Object -Maximum).Maximum
    $passed = $p95 -le $TargetMs
    $status = if ($passed) { "passed" } else { "needs_tuning" }

    return [pscustomobject]@{
        mode = $Mode
        target_latency_proxy_ms = $TargetMs
        sample_count = $modeSamples.Count
        passed = $passed
        status = $status
        average_latency_proxy_ms = $average
        p95_latency_proxy_ms = $p95
        max_latency_proxy_ms = $max
    }
}

$targets = [ordered]@{
    low_latency = 120
    balanced = 200
    high_quality = 350
}

$jsonFiles = Get-JsonFiles -Paths $SnapshotPath
$samples = New-Object System.Collections.Generic.List[object]

foreach ($file in $jsonFiles) {
    $document = Get-Content -Raw -LiteralPath $file | ConvertFrom-Json
    $items = if ($document -is [array]) { $document } else { @($document) }
    foreach ($item in $items) {
        $serviceSnapshot = Get-ServiceSnapshot -Document $item
        if ($null -eq $serviceSnapshot) {
            continue
        }
        $sample = New-Sample -SourceFile $file -ServiceSnapshot $serviceSnapshot
        if (-not [string]::IsNullOrWhiteSpace($sample.mode)) {
            $samples.Add($sample)
        }
    }
}

$modeResults = New-Object System.Collections.Generic.List[object]
foreach ($mode in $targets.Keys) {
    $modeResults.Add((New-ModeResult -Mode $mode -TargetMs $targets[$mode] -Samples $samples))
}

$overallPassed = ($modeResults | Where-Object { -not $_.passed }).Count -eq 0
$overallStatus = if ($overallPassed) {
    "passed"
} elseif ($samples.Count -eq 0) {
    "pending_no_snapshot"
} else {
    "needs_tuning"
}

$artifact = [pscustomobject]@{
    contract_version = 1
    generated_at = (Get-NowIso)
    source = @{
        snapshot_paths = $SnapshotPath
        snapshot_files_read = @($jsonFiles)
        metric_formula = "latency_proxy_ms = buffered_ms + rtt_ms + sink_write_gap_ms_p95"
    }
    targets = $targets
    overall = @{
        passed = $overallPassed
        status = $overallStatus
        sample_count = $samples.Count
    }
    modes = $modeResults.ToArray()
    samples = $samples.ToArray()
}

if ([System.IO.Path]::IsPathRooted($OutputPath)) {
    $resolvedOutput = $OutputPath
} else {
    $resolvedOutput = Join-Path (Get-Location) $OutputPath
}
$outputDir = Split-Path -Parent $resolvedOutput
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
($artifact | ConvertTo-Json -Depth 8) | Set-Content -LiteralPath $resolvedOutput -Encoding UTF8

Write-Host "Latency probe artifact written: $resolvedOutput"
Write-Host "Status: $($artifact.overall.status); samples=$($artifact.overall.sample_count)"
