use crate::commands::disk::DiskInfo;
use crate::commands::disk::DiskDiagnostics;
use crate::utils::command::CommandExecutor;
use crate::{AppError, Result};
use tracing::info;

/// PowerShell script to get disk info with volume letters.
/// For each disk, we query its partitions → volumes → drive letters.
const PS_LIST_DISKS: &str = r#"
$physical = Get-PhysicalDisk | Select-Object FriendlyName, MediaType, SpindleSpeed
$disks = Get-Disk | Select-Object Number, FriendlyName, Size, BusType, MediaType, IsBoot, IsSystem
$result = @()
foreach ($d in $disks) {
    # Try to resolve media type (SSD/HDD)
    $pd = $physical | Where-Object { $_.FriendlyName -eq $d.FriendlyName } | Select-Object -First 1
    $media = $d.MediaType
    if (-not $media -or $media -eq 'Unspecified' -or $media -eq '') {
        if ($pd) { $media = $pd.MediaType }
    }
    $busRaw = [string]$d.BusType
    if (-not $media -or $media -eq 'Unspecified' -or $media -eq '') {
        if ($busRaw -match '(^17$|NVMe)') {
            $media = 'SSD'
        } elseif ($pd -and $pd.SpindleSpeed -eq 0) {
            $media = 'SSD'
        } elseif ($pd -and $pd.SpindleSpeed -gt 0) {
            $media = 'HDD'
        } elseif ($d.FriendlyName -match '(?i)SSD|NVME') {
            $media = 'SSD'
        } else {
            $media = 'Unknown'
        }
    }

    $volumes = Get-Partition -DiskNumber $d.Number -ErrorAction SilentlyContinue |
        Get-Volume -ErrorAction SilentlyContinue |
        Where-Object { $_.DriveLetter -ne $null -and $_.DriveLetter -ne '' } |
        Select-Object -ExpandProperty DriveLetter
    $vol = if ($volumes -is [array]) { $volumes[0] } else { $volumes }
    $busName = switch -Regex ($busRaw) {
        '^0$|^Unknown$' { "Unknown" }
        '^1$|^SCSI$' { "SCSI" }
        '^2$|^ATAPI$' { "ATAPI" }
        '^3$|^ATA$' { "ATA" }
        '^4$|^1394$' { "1394" }
        '^5$|^SSA$' { "SSA" }
        '^6$|^FibreChannel$' { "FibreChannel" }
        '^7$|^USB$' { "USB" }
        '^8$|^RAID$' { "RAID" }
        '^9$|^iSCSI$' { "iSCSI" }
        '^10$|^SAS$' { "SAS" }
        '^11$|^SATA$' { "SATA" }
        '^12$|^SD$' { "SD" }
        '^13$|^MMC$' { "MMC" }
        '^15$|^Virtual$' { "Virtual" }
        '^16$|^StorageSpaces$' { "StorageSpaces" }
        '^17$|^NVMe$' { "NVMe" }
        default { if ($busRaw) { $busRaw } else { "Other" } }
    }
    $result += [PSCustomObject]@{
        Number       = $d.Number
        FriendlyName = $d.FriendlyName
        Size         = $d.Size
        BusType      = $d.BusType
        MediaType    = $media
        IsBoot       = $d.IsBoot
        IsSystem     = $d.IsSystem
        BusTypeName  = $busName
        VolumeLetter = if ($vol) { [string]$vol } else { "" }
    }
}
$result | ConvertTo-Json
"#;

/// PowerShell script to get detailed disk diagnostics including SMART/reliability.
const PS_LIST_DISK_DIAGNOSTICS: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'

function Normalize-Text([object]$value) {
    if ($null -eq $value) { return '' }
    return ([string]$value).Trim()
}

function Get-BusTypeName([object]$bus) {
    $raw = Normalize-Text $bus
    switch -Regex ($raw) {
        '^0$|^Unknown$' { 'Unknown' }
        '^1$|^SCSI$' { 'SCSI' }
        '^2$|^ATAPI$' { 'ATAPI' }
        '^3$|^ATA$' { 'ATA' }
        '^4$|^1394$' { '1394' }
        '^5$|^SSA$' { 'SSA' }
        '^6$|^FibreChannel$' { 'FibreChannel' }
        '^7$|^USB$' { 'USB' }
        '^8$|^RAID$' { 'RAID' }
        '^9$|^iSCSI$' { 'iSCSI' }
        '^10$|^SAS$' { 'SAS' }
        '^11$|^SATA$' { 'SATA' }
        '^12$|^SD$' { 'SD' }
        '^13$|^MMC$' { 'MMC' }
        '^15$|^Virtual$' { 'Virtual' }
        '^16$|^StorageSpaces$' { 'StorageSpaces' }
        '^17$|^NVMe$' { 'NVMe' }
        default { if ($raw) { $raw } else { 'Unknown' } }
    }
}

function Is-UsbDevice([string]$busName, [string]$interfaceType, [string]$pnp) {
    return ($busName -eq 'USB') -or ($interfaceType -eq 'USB') -or ($pnp -like 'USB*') -or ($pnp -like '*USBSTOR*')
}

function Resolve-MediaType([string]$rawMedia, [object]$physicalDisk, [string]$busName, [string]$model, [string]$interfaceType) {
    $media = Normalize-Text $rawMedia
    if (-not $media -or $media -eq 'Unspecified') {
        if ($physicalDisk -and $physicalDisk.MediaType) {
            $media = Normalize-Text $physicalDisk.MediaType
        }
    }

    if ($media -and $media -ne 'Unspecified') {
        if ($media -match '(?i)SSD') { return 'SSD' }
        if ($media -match '(?i)HDD|Rotational') { return 'HDD' }
        return $media
    }

    if ($busName -eq 'NVMe' -or $model -match '(?i)NVME|SSD' -or $interfaceType -eq 'NVMExpress') {
        return 'SSD'
    }

    if ($physicalDisk -and $null -ne $physicalDisk.SpindleSpeed) {
        if ([int64]$physicalDisk.SpindleSpeed -eq 0) { return 'SSD' }
        if ([int64]$physicalDisk.SpindleSpeed -gt 0) { return 'HDD' }
    }

    if ($model -match '(?i)\bHDD\b|ST\d{3,}|WDC|WD\d{3,}|TOSHIBA|HITACHI') {
        return 'HDD'
    }

    return 'Unknown'
}

function Test-MaskedSerial([string]$serial) {
    $v = Normalize-Text $serial
    if (-not $v) { return $true }

    $normalized = ($v -replace '[^0-9A-Za-z]', '').ToUpperInvariant()
    if (-not $normalized) { return $true }
    if ($normalized.Length -lt 4) { return $true }
    if ($normalized -match '^[0]+$') { return $true }
    if ($normalized -match '^[F]+$') { return $true }
    if ($normalized -match '^[D0]+$') { return $true }
    if ($normalized -match '^0{8,}D0{8,}$') { return $true }
    return $false
}

function Get-FirstValidSerial([object[]]$candidates) {
    foreach ($candidate in $candidates) {
        $serial = Normalize-Text $candidate
        if (-not $serial) { continue }
        if (-not (Test-MaskedSerial $serial)) {
            return $serial
        }
    }
    return ''
}

function Get-SmartName([int]$id) {
    switch ($id) {
        1 { 'Read Error Rate' }
        2 { 'Throughput Performance' }
        3 { 'Spin-Up Time' }
        4 { 'Start/Stop Count' }
        5 { 'Reallocated Sectors Count' }
        7 { 'Seek Error Rate' }
        8 { 'Seek Time Performance' }
        9 { 'Power-On Hours' }
        10 { 'Spin Retry Count' }
        11 { 'Calibration Retry Count' }
        12 { 'Power Cycle Count' }
        170 { 'Available Reserved Space' }
        171 { 'Program Fail Count' }
        172 { 'Erase Fail Count' }
        173 { 'Wear Leveling Count' }
        174 { 'Unexpected Power Loss Count' }
        177 { 'Wear Range Delta' }
        179 { 'Used Reserved Block Count Total' }
        180 { 'Unused Reserved Block Count Total' }
        181 { 'Program Fail Count Total' }
        182 { 'Erase Fail Count' }
        183 { 'Runtime Bad Block' }
        184 { 'End-to-End Error' }
        187 { 'Reported Uncorrectable Errors' }
        188 { 'Command Timeout' }
        190 { 'Airflow Temperature' }
        191 { 'G-Sense Error Rate' }
        192 { 'Power-Off Retract Count' }
        193 { 'Load/Unload Cycle Count' }
        194 { 'Temperature' }
        195 { 'Hardware ECC Recovered' }
        196 { 'Reallocation Event Count' }
        197 { 'Current Pending Sector Count' }
        198 { 'Offline Uncorrectable Sector Count' }
        199 { 'UltraDMA CRC Error Count' }
        200 { 'Write Error Rate' }
        201 { 'Soft Read Error Rate' }
        202 { 'Data Address Mark Errors' }
        206 { 'Flying Height' }
        210 { 'Vibration During Write' }
        211 { 'Vibration During Write Time' }
        212 { 'Shock During Write' }
        225 { 'Load/Unload Retry Count' }
        226 { 'Load-in Time' }
        227 { 'Torque Amplification Count' }
        228 { 'Power-Off Retract Cycle' }
        230 { 'Drive Life Protection Status' }
        231 { 'SSD Life Left' }
        232 { 'Available Reserved Space' }
        233 { 'Media Wearout Indicator' }
        234 { 'Average Erase Count' }
        241 { 'Total LBAs Written' }
        242 { 'Total LBAs Read' }
        246 { 'Total NAND Writes' }
        247 { 'Host Program NAND Pages Count' }
        248 { 'FTL Program NAND Pages Count' }
        249 { 'NAND Writes 1GiB' }
        250 { 'Read Error Retry Rate' }
        default { "Attribute $id" }
    }
}

function Parse-SmartAttributes([byte[]]$vendor, [byte[]]$thresholds) {
    $attrs = @()
    if (-not $vendor -or $vendor.Length -lt 362) { return @() }
    for ($i = 2; $i -lt 362; $i += 12) {
        $id = [int]$vendor[$i]
        if ($id -eq 0) { continue }

        $current = [int]$vendor[$i + 3]
        $worst = [int]$vendor[$i + 4]
        [uint64]$raw = 0
        for ($j = 0; $j -lt 6; $j++) {
            $raw += ([uint64]$vendor[$i + 5 + $j] -shl (8 * $j))
        }

        $threshold = $null
        if ($thresholds -and $thresholds.Length -ge 362) {
            for ($k = 2; $k -lt 362; $k += 12) {
                if ([int]$thresholds[$k] -eq $id) {
                    $threshold = [int]$thresholds[$k + 1]
                    break
                }
            }
        }

        $attrs += [PSCustomObject]@{
            id = $id
            name = Get-SmartName $id
            current = $current
            worst = $worst
            threshold = $threshold
            raw = $raw
            raw_hex = ('0x{0:X12}' -f $raw)
        }
    }
    return $attrs
}

function Get-SmartAttr([object[]]$attrs, [int]$id) {
    if (-not $attrs) { return $null }
    return $attrs | Where-Object { [int]$_.id -eq $id } | Select-Object -First 1
}

function Get-SmartRaw([object[]]$attrs, [int]$id) {
    $attr = Get-SmartAttr $attrs $id
    if ($attr -and $null -ne $attr.raw) {
        return [uint64]$attr.raw
    }
    return $null
}

function Get-SmartCurrent([object[]]$attrs, [int]$id) {
    $attr = Get-SmartAttr $attrs $id
    if ($attr -and $null -ne $attr.current) {
        return [int]$attr.current
    }
    return $null
}

function Get-FirstSmartRaw([object[]]$attrs, [int[]]$ids) {
    foreach ($id in $ids) {
        $raw = Get-SmartRaw $attrs $id
        if ($null -ne $raw) { return [uint64]$raw }
    }
    return $null
}

function Get-TemperatureFromRaw([object]$raw) {
    if ($null -eq $raw) { return $null }
    try {
        $value = [uint64]$raw
    } catch {
        return $null
    }
    if ($value -le 200) { return [double]$value }
    return [double]($value -band 0xFF)
}

$disks = Get-Disk | Sort-Object Number
$physical = Get-PhysicalDisk
$win32 = Get-CimInstance Win32_DiskDrive
$physicalMedia = Get-CimInstance Win32_PhysicalMedia
$fpStatusAll = Get-WmiObject -Namespace root\wmi -Class MSStorageDriver_FailurePredictStatus
$fpDataAll = Get-WmiObject -Namespace root\wmi -Class MSStorageDriver_FailurePredictData
$fpThresholdAll = Get-WmiObject -Namespace root\wmi -Class MSStorageDriver_FailurePredictThresholds
$result = @()

foreach ($d in $disks) {
    $wd = $win32 | Where-Object { $_.Index -eq $d.Number } | Select-Object -First 1
    $uniqueId = Normalize-Text $d.UniqueId
    $model = if ($wd) { Normalize-Text $wd.Model } else { '' }
    if (-not $model) { $model = Normalize-Text $d.FriendlyName }
    $firmware = if ($wd) { Normalize-Text $wd.FirmwareRevision } else { '' }
    $busTypeName = Get-BusTypeName $d.BusType
    $pnp = if ($wd) { Normalize-Text $wd.PNPDeviceID } else { '' }
    $interfaceType = if ($wd) { Normalize-Text $wd.InterfaceType } else { '' }
    $isUsb = Is-UsbDevice $busTypeName $interfaceType $pnp
    $transport = switch -Regex ($busTypeName) {
        '^NVMe$' { 'NVMe' }
        '^SATA$|^ATA$' { 'SATA/ATA' }
        '^USB$' { 'USB' }
        '^SAS$' { 'SAS' }
        '^RAID$' { 'RAID' }
        '^SD$' { 'SD' }
        default { if ($isUsb) { 'USB' } else { $busTypeName } }
    }
    $iface = if ($transport -eq 'NVMe') { 'NVMExpress' } elseif ($isUsb) { 'USB' } elseif ($interfaceType) { $interfaceType } else { $busTypeName }

    $pd = $physical | Where-Object {
        ($_.DeviceId -eq $d.Number) -or
        ($_.FriendlyName -eq $d.FriendlyName)
    } | Select-Object -First 1

    if (-not $pd) {
        $pd = $physical | Where-Object { [uint64]$_.Size -eq [uint64]$d.Size } | Select-Object -First 1
    }

    $pm = $null
    if ($wd -and $wd.DeviceID) {
        $wdDeviceId = Normalize-Text $wd.DeviceID
        $pm = $physicalMedia | Where-Object { (Normalize-Text $_.Tag) -eq $wdDeviceId } | Select-Object -First 1
    }

    $serialCandidates = @(
        $(if ($wd) { Normalize-Text $wd.SerialNumber } else { '' }),
        $(Normalize-Text $d.SerialNumber),
        $(if ($pd) { Normalize-Text $pd.SerialNumber } else { '' }),
        $(if ($pm) { Normalize-Text $pm.SerialNumber } else { '' })
    )
    $serial = Get-FirstValidSerial $serialCandidates
    $serialMasked = $false
    if (-not $serial) {
        $serialMasked = [bool](($serialCandidates | Where-Object { $_ -and (Test-MaskedSerial $_) } | Select-Object -First 1))
    }

    if ($pd) {
        if (-not $firmware) { $firmware = Normalize-Text $pd.FirmwareVersion }
        if (-not $model) { $model = Normalize-Text $pd.FriendlyName }
    }

    $media = Resolve-MediaType (Normalize-Text $d.MediaType) $pd $busTypeName $model $iface
    $health = if ($pd -and $pd.HealthStatus) { [string]$pd.HealthStatus } else { 'Unknown' }

    $reliability = $null
    if ($pd) {
        try {
            $reliability = Get-StorageReliabilityCounter -PhysicalDisk $pd -ErrorAction Stop
        } catch {}
    }

    $fpStatus = $null
    $fpData = $null
    $fpThresholds = $null
    if ($pnp) {
        $instance = $pnp.Replace('\', '_')
        $fpStatus = $fpStatusAll | Where-Object { $_.InstanceName -like "*$instance*" } | Select-Object -First 1
        $fpData = $fpDataAll | Where-Object { $_.InstanceName -like "*$instance*" } | Select-Object -First 1
        $fpThresholds = $fpThresholdAll | Where-Object { $_.InstanceName -like "*$instance*" } | Select-Object -First 1
    }

    $attrs = @()
    if ($fpData -and $fpData.VendorSpecific) {
        $thresholdBytes = if ($fpThresholds) { $fpThresholds.VendorSpecific } else { $null }
        $attrs = Parse-SmartAttributes -vendor $fpData.VendorSpecific -thresholds $thresholdBytes
    }

    $ataSmartAvailable = [bool]($attrs -and $attrs.Count -gt 0)
    $reliabilityAvailable = [bool]$reliability
    $smartSupported = $ataSmartAvailable -or $reliabilityAvailable -or [bool]$fpStatus
    $smartEnabled = if ($fpStatus) { [bool](-not $fpStatus.PredictFailure) } elseif ($smartSupported) { $true } else { $false }
    $smartSource = if ($ataSmartAvailable) { 'ATA_SMART_WMI' } elseif ($reliabilityAvailable) { 'STORAGE_RELIABILITY' } else { 'NONE' }

    $temperature = if ($reliability -and $null -ne $reliability.Temperature) { [double]$reliability.Temperature } else { $null }
    if ($null -eq $temperature) { $temperature = Get-TemperatureFromRaw (Get-SmartRaw $attrs 194) }
    if ($null -eq $temperature) { $temperature = Get-TemperatureFromRaw (Get-SmartRaw $attrs 190) }

    $powerOnHours = if ($reliability -and $null -ne $reliability.PowerOnHours) { [uint64]$reliability.PowerOnHours } else { Get-SmartRaw $attrs 9 }
    $powerCycleCount = if ($reliability -and $null -ne $reliability.PowerCycleCount) { [uint64]$reliability.PowerCycleCount } else { Get-SmartRaw $attrs 12 }

    $wear = if ($reliability -and $null -ne $reliability.Wear) { [double]$reliability.Wear } else { $null }
    if ($null -eq $wear) {
        $lifeLeft = Get-SmartCurrent $attrs 231
        if ($null -eq $lifeLeft) { $lifeLeft = Get-SmartCurrent $attrs 233 }
        if ($null -eq $lifeLeft) { $lifeLeft = Get-SmartCurrent $attrs 202 }
        if ($null -ne $lifeLeft) {
            $wear = [double]([Math]::Max([Math]::Min(100 - [double]$lifeLeft, 100), 0))
        }
    }

    $readErrorsTotal = if ($reliability -and $null -ne $reliability.ReadErrorsTotal) { [uint64]$reliability.ReadErrorsTotal } else { Get-FirstSmartRaw $attrs @(1, 187, 201) }
    $writeErrorsTotal = if ($reliability -and $null -ne $reliability.WriteErrorsTotal) { [uint64]$reliability.WriteErrorsTotal } else { Get-FirstSmartRaw $attrs @(200, 181, 171) }
    $hostWritesTotal = if ($reliability -and $null -ne $reliability.HostWritesTotal) { [uint64]$reliability.HostWritesTotal } else { Get-SmartRaw $attrs 241 }
    $hostReadsTotal = if ($reliability -and $null -ne $reliability.HostReadsTotal) { [uint64]$reliability.HostReadsTotal } else { Get-SmartRaw $attrs 242 }

    $reliabilityObj = $null
    if ($reliability) {
        $reliabilityObj = [PSCustomObject]@{
            Temperature = $reliability.Temperature
            PowerOnHours = $reliability.PowerOnHours
            PowerCycleCount = $reliability.PowerCycleCount
            Wear = $reliability.Wear
            ReadErrorsTotal = $reliability.ReadErrorsTotal
            WriteErrorsTotal = $reliability.WriteErrorsTotal
            ReadErrorsUncorrected = $reliability.ReadErrorsUncorrected
            WriteErrorsUncorrected = $reliability.WriteErrorsUncorrected
            HostReadsTotal = $reliability.HostReadsTotal
            HostWritesTotal = $reliability.HostWritesTotal
        }
    }

    $notes = @()
    if ($ataSmartAvailable -and $reliabilityAvailable) {
        $notes += 'Using ATA SMART attributes and Storage Reliability counters.'
    } elseif ($ataSmartAvailable) {
        $notes += 'Using ATA SMART attribute table.'
    } elseif ($reliabilityAvailable) {
        $notes += 'ATA SMART attribute table unavailable; using Storage Reliability counters.'
    } else {
        $notes += 'SMART/reliability counters unavailable for this device path (common with some USB bridges/RAID drivers).'
    }
    if ($serialMasked) {
        $notes += 'Serial number appears masked by controller/driver; another identifier may be required for exact model matching.'
    } elseif (-not $serial) {
        $notes += 'No usable serial number returned by current Windows APIs for this device.'
    }
    if ($ataSmartAvailable -and -not $reliabilityAvailable) {
        $notes += 'Some counters were derived from ATA SMART attributes because Storage Reliability counters were incomplete.'
    }
    if ($isUsb -and -not $ataSmartAvailable -and -not $reliabilityAvailable) {
        $notes += 'USB bridge may block pass-through SMART commands on this enclosure.'
    }

    $result += [PSCustomObject]@{
        id = "disk$($d.Number)"
        disk_number = [int]$d.Number
        model = $model
        friendly_name = [string]$d.FriendlyName
        serial_number = $serial
        firmware_version = $firmware
        interface_type = $iface
        transport_type = $transport
        is_usb = [bool]$isUsb
        bus_type = $busTypeName
        unique_id = $uniqueId
        media_type = $media
        size_bytes = [uint64]$d.Size
        is_system = [bool]($d.IsBoot -or $d.IsSystem)
        health_status = $health
        smart_supported = $smartSupported
        smart_enabled = $smartEnabled
        smart_data_source = $smartSource
        ata_smart_available = [bool]$ataSmartAvailable
        reliability_available = [bool]$reliabilityAvailable
        temperature_c = $temperature
        power_on_hours = $powerOnHours
        power_cycle_count = $powerCycleCount
        percentage_used = $wear
        read_errors_total = $readErrorsTotal
        write_errors_total = $writeErrorsTotal
        host_reads_total = $hostReadsTotal
        host_writes_total = $hostWritesTotal
        smart_attributes = $attrs
        reliability = $reliabilityObj
        notes = $notes
    }
}

$result | ConvertTo-Json -Depth 8
"#;

/// List all disks on Windows using PowerShell
pub async fn list_disks() -> Result<Vec<DiskInfo>> {
    let output = CommandExecutor::execute_allow_fail(
        "powershell.exe",
        &["-NoProfile", "-Command", PS_LIST_DISKS],
    )?;

    let trimmed = output.trim();

    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    // Find the JSON portion (skip any non-JSON output before it)
    let json_start = trimmed.find('[').or_else(|| trimmed.find('{'));
    let json_str = match json_start {
        Some(pos) => &trimmed[pos..],
        None => {
            info!("No JSON found in PowerShell output: {}", &trimmed[..trimmed.len().min(200)]);
            return Ok(vec![]);
        }
    };

    let disks: Vec<DiskInfo> = if json_str.starts_with('[') {
        let raw: Vec<serde_json::Value> = serde_json::from_str(json_str)
            .map_err(|e| AppError::JsonError(format!("{}: {}", e, &json_str[..json_str.len().min(300)])))?;
        raw.iter().map(|d| parse_disk(d)).collect()
    } else {
        let raw: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| AppError::JsonError(format!("{}: {}", e, &json_str[..json_str.len().min(300)])))?;
        vec![parse_disk(&raw)]
    };

    info!("Found {} disks", disks.len());
    Ok(disks)
}

/// List detailed disk diagnostics on Windows
pub async fn list_disk_diagnostics() -> Result<Vec<DiskDiagnostics>> {
    let output = CommandExecutor::execute_allow_fail(
        "powershell.exe",
        &["-NoProfile", "-Command", PS_LIST_DISK_DIAGNOSTICS],
    )?;

    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    let json_start = trimmed.find('[').or_else(|| trimmed.find('{'));
    let json_str = match json_start {
        Some(pos) => &trimmed[pos..],
        None => {
            info!(
                "No diagnostics JSON found in PowerShell output: {}",
                &trimmed[..trimmed.len().min(300)]
            );
            return Ok(vec![]);
        }
    };

    let diagnostics: Vec<DiskDiagnostics> = if json_str.starts_with('[') {
        serde_json::from_str(json_str).map_err(|e| {
            AppError::JsonError(format!(
                "{}: {}",
                e,
                &json_str[..json_str.len().min(600)]
            ))
        })?
    } else {
        vec![serde_json::from_str(json_str).map_err(|e| {
            AppError::JsonError(format!(
                "{}: {}",
                e,
                &json_str[..json_str.len().min(600)]
            ))
        })?]
    };

    Ok(diagnostics)
}

fn parse_disk(v: &serde_json::Value) -> DiskInfo {
    let number = v["Number"].as_u64().unwrap_or(0);
    let name = v["FriendlyName"].as_str().unwrap_or("Unknown").to_string();
    let size = v["Size"].as_u64().unwrap_or(0);
    let bus_type = v["BusType"].as_u64().unwrap_or(0);
    let media_type_raw = v["MediaType"].as_str().unwrap_or("").to_string();
    let is_system = v["IsSystem"].as_bool().unwrap_or(false) || v["IsBoot"].as_bool().unwrap_or(false);
    let bus_name = v["BusTypeName"]
        .as_str()
        .or_else(|| v["BusType"].as_str())
        .unwrap_or("Other")
        .to_string();
    // Removable buses: USB/SD/MMC
    let removable = if bus_type == 7 || bus_type == 12 || bus_type == 13 {
        true
    } else {
        let bus_up = bus_name.to_uppercase();
        bus_up.contains("USB") || bus_up == "SD" || bus_up == "MMC"
    };
    let device = format!("PhysicalDrive{}", number);

    let drive_type = if removable {
        "Removable".to_string()
    } else {
        format!("Fixed ({})", bus_name)
    };

    let volume = v["VolumeLetter"].as_str().unwrap_or("").to_string();

    DiskInfo {
        id: format!("disk{}", number),
        name,
        size,
        removable,
        device,
        drive_type,
        media_type: normalize_media_type(&media_type_raw),
        index: number.to_string(),
        volume,
        is_system,
    }
}

fn normalize_media_type(raw: &str) -> String {
    let up = raw.to_uppercase();
    if up.contains("SSD") || up.contains("NVME") || up == "4" {
        "SSD".to_string()
    } else if up.contains("HDD") || up.contains("ROTATIONAL") || up == "3" {
        "HDD".to_string()
    } else if up.contains("UNKNOWN") || up.contains("UNSPECIFIED") || up.trim().is_empty() {
        "Unknown".to_string()
    } else {
        raw.trim().to_string()
    }
}

/// Get disk info on Windows
pub async fn get_disk_info(disk_id: &str) -> Result<DiskInfo> {
    let disks = list_disks().await?;
    disks
        .into_iter()
        .find(|d| d.id == disk_id)
        .ok_or_else(|| AppError::DeviceNotFound(disk_id.to_string()))
}

/// Start USB monitoring on Windows
pub async fn start_usb_monitoring(_app_handle: tauri::AppHandle) -> Result<String> {
    Ok("monitor-windows".to_string())
}

/// Stop USB monitoring on Windows
pub async fn stop_usb_monitoring(_monitor_id: &str) -> Result<()> {
    Ok(())
}
